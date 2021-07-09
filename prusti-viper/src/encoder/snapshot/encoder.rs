// © 2021, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ::log::{debug, trace};
use num_traits::Float;
use prusti_common::vir::FloatSize;
use rustc_middle::ty;
use rustc_middle::ty::layout::IntegerExt;
use rustc_target::abi::Integer;
use std::{
    collections::HashMap,
    convert::TryInto,
};
use prusti_common::{
    vir,
    vir_local,
    vir::{
        Expr, FallibleExprFolder, FallibleStmtFolder, Type, PermAmount,
        EnumVariantIndex, ExprIterator, ContainerOpKind,
    },
};
use crate::encoder::{
    Encoder,
    array_encoder::EncodedArrayTypes,
    foldunfold,
    utils::range_extract,
    errors::{EncodingError, EncodingResult, SpannedEncodingResult},
    snapshot::{Snapshot, patcher::SnapshotPatcher},
    builtin_encoder::BuiltinFunctionKind,
};

type PredicateName = String;

pub(super) const UNIT_DOMAIN_NAME: &str = "UnitDomain";
const SNAP_FUNC_NAME: &str = "snap$";

/// Encodes MIR types into snapshots, and keeps track of which types have
/// already been encoded.
pub struct SnapshotEncoder {
    /// Types that are currently being encoded. Stores the VIR type that will
    /// be the result of the snapshot operation: either a primitive VIR type,
    /// or an instance of [Type::Snapshot].
    ///
    /// This is necessary to be able to encode recursive types; when encoding
    /// a snapshot where once of the fields is the same type, it is enough to
    /// look up the resulting type from this hashmap.
    in_progress: HashMap<PredicateName, Type>,

    /// Maps predicate names to encoded snapshots.
    encoded: HashMap<PredicateName, Snapshot>,

    /// Whether the unit domain was used in encoding or not.
    unit_used: bool,
    unit_domain: vir::Domain,
}

/// Snapshot encoding flattens references and boxes. This function removes any
/// [Box<...>] or reference (mutable or shared) wrappers.
fn strip_refs_and_boxes<'tcx>(ty: ty::Ty<'tcx>) -> ty::Ty<'tcx> {
    match ty.kind() {
        _ if ty.is_box() => strip_refs_and_boxes(ty.boxed_ty()),
        ty::TyKind::Ref(_, ref sub_ty, _) => strip_refs_and_boxes(sub_ty),
        _ => ty,
    }
}

/// Same as [strip_refs_and_boxes], but also performs the needed field accesses
/// on the given expression to deref/unbox it.
fn strip_refs_and_boxes_expr<'p, 'v: 'p, 'tcx: 'v>(
    encoder: &'p Encoder<'v, 'tcx>,
    ty: ty::Ty<'tcx>,
    expr: Expr,
) -> EncodingResult<(ty::Ty<'tcx>, Expr)> {
    match ty.kind() {
        _ if ty.is_box() => strip_refs_and_boxes_expr(
            encoder,
            ty.boxed_ty(),
            Expr::field(
                expr,
                encoder.encode_dereference_field(ty.boxed_ty())?,
            ),
        ),
        ty::TyKind::Ref(_, ref sub_ty, _) => strip_refs_and_boxes_expr(
            encoder,
            sub_ty,
            Expr::field(
                expr,
                encoder.encode_dereference_field(sub_ty)?,
            ),
        ),
        _ => Ok((ty, expr)),
    }
}

/// Returns a `forall` quantifier if `vars` is not empty, otherwise returns
/// the `body` directly.
fn forall_or_body(
    vars: Vec<vir::LocalVar>,
    triggers: Vec<vir::Trigger>,
    body: Expr,
) -> Expr {
    if vars.is_empty() {
        body
    } else {
        Expr::forall(vars, triggers, body)
    }
}

impl SnapshotEncoder {
    pub fn new() -> Self {
        Self {
            in_progress: HashMap::new(),
            encoded: HashMap::new(),
            unit_used: false,
            unit_domain: vir::Domain {
                name: UNIT_DOMAIN_NAME.to_string(),
                functions: vec![vir::DomainFunc {
                    name: "unit$".to_string(),
                    formal_args: vec![],
                    return_type: Type::Domain(UNIT_DOMAIN_NAME.to_string()),
                    unique: false,
                    domain_name: UNIT_DOMAIN_NAME.to_string(),
                }],
                axioms: vec![],
                type_vars: vec![],
            },
        }
    }

    /// Returns a list of Viper functions needed by the encoded snapshots.
    pub fn get_viper_functions(&self) -> Vec<vir::Function> {
        let mut funcs = vec![];
        for snapshot in self.encoded.values() {
            match snapshot {
                Snapshot::Complex { snap_func, .. }
                | Snapshot::Abstract { snap_func, .. }
                | Snapshot::Array { snap_func, .. } => funcs.push(snap_func.clone()),
                _ => {},
            }
        }
        funcs
    }

    /// Returns a list of Viper domains needed by the encoded snapshots.
    pub fn get_viper_domains(&self) -> Vec<vir::Domain> {
        let mut domains = vec![];
        for snapshot in self.encoded.values() {
            match snapshot {
                Snapshot::Complex { domain, .. }
                | Snapshot::Abstract { domain, .. }
                | Snapshot::Array { domain, .. } => domains.push(domain.clone()),
                _ => {},
            }
        }
        if self.unit_used {
            domains.push(self.unit_domain.clone());
        }
        domains
    }

    /// Patches snapshots in a method.
    pub fn patch_snapshots_method<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        method: vir::CfgMethod,
    ) -> EncodingResult<vir::CfgMethod> {
        debug!("[snap] method: {:?}", method.name());
        let mut patcher = SnapshotPatcher {
            snapshot_encoder: self,
            encoder,
        };
        method.patch_statements(|stmt| vir::FallibleStmtFolder::fallible_fold(&mut patcher, stmt))
    }

    /// Patches snapshots in a function.
    pub fn patch_snapshots_function<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        mut function: vir::Function,
    ) -> EncodingResult<vir::Function> {
        debug!("[snap] function: {:?}", function.name);
        let mut patcher = SnapshotPatcher {
            snapshot_encoder: self,
            encoder,
        };
        function.pres = function.pres.into_iter()
            .map(|e| FallibleExprFolder::fallible_fold(&mut patcher, e))
            .collect::<Result<Vec<_>, _>>()?;
        function.posts = function.posts.into_iter()
            .map(|e| FallibleExprFolder::fallible_fold(&mut patcher, e))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(body) = function.body {
            function.body = Some(FallibleExprFolder::fallible_fold(&mut patcher, body)?);
        }
        Ok(function)
    }

    /// Patches snapshots in an expression.
    pub fn patch_snapshots_expr<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        expr: Expr,
    ) -> EncodingResult<Expr> {
        debug!("[snap] expr: {:?}", expr);
        let mut patcher = SnapshotPatcher {
            snapshot_encoder: self,
            encoder,
        };
        FallibleExprFolder::fallible_fold(&mut patcher, expr)
    }

    /// Converts a SnapApp(expr) into an appropriate expression based on the
    /// kind of this snapshot.
    pub(super) fn snap_app<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        expr: Expr,
    ) -> EncodingResult<Expr> {
        match expr.get_type() {
            vir::Type::TypedRef(ref name) => {
                let ty = encoder.decode_type_predicate(name)?;
                let (ty, expr) = strip_refs_and_boxes_expr(encoder, ty, expr)?;
                Ok(match ty.kind() {
                    ty::TyKind::Int(_)
                    | ty::TyKind::Uint(_)
                    | ty::TyKind::Char => Expr::field(
                        expr.clone(),
                        vir::Field::new("val_int", Type::Int),
                    ),
                    ty::TyKind::Bool => Expr::field(
                        expr.clone(),
                        vir::Field::new("val_bool", Type::Bool),
                    ),
                    ty::TyKind::Float(ty::FloatTy::F32) => Expr::field(
                        expr.clone(),
                        vir::Field::new("val_float32", Type::Float(FloatSize::F32)),
                    ),
                    ty::TyKind::Float(ty::FloatTy::F64) => Expr::field(
                        expr.clone(),
                        vir::Field::new("val_float64", Type::Float(FloatSize::F64)),
                    ),
                    ty::TyKind::Tuple(substs) if substs.is_empty() => self.snap_unit(),
                    ty::TyKind::Adt(adt_def, _) if adt_def.variants.is_empty() => self.snap_unit(),
                    ty::TyKind::Adt(adt_def, _) if adt_def.variants.len() == 1 && adt_def.variants[rustc_target::abi::VariantIdx::from_u32(0)].fields.is_empty() => self.snap_unit(),

                    // Param(_) | Adt(_) | Tuple(_) and unsupported types
                    _ => {
                        let snapshot = self.encode_snapshot(encoder, ty)?;
                        self.snap_app_expr(expr, snapshot.get_type())
                    }
                })
            }

            // handle SnapApp on already patched expressions
            vir::Type::Domain(dom) if dom == UNIT_DOMAIN_NAME => Ok(expr),
            vir::Type::Snapshot(_)
            | vir::Type::Bool // TODO: restrict to snapshot-produced Bools and Ints
            | vir::Type::Int
            | vir::Type::Float(_) => Ok(expr),

            _ => Err(EncodingError::internal(
                format!("SnapApp applied to expr of invalid type {:?}", expr),
            )),
        }
    }

    /// Calls the [snap] function on the given expression. This should only
    /// ever be used when [expr] has been snapshot-encoded and the snapshot has
    /// a snap function (i.e. it is Complex or Abstract).
    fn snap_app_expr(
        &self,
        expr: Expr,
        ty: vir::Type,
    ) -> Expr {
        // here we rely on the name being the same for all complex types rather
        // than looking up the snapshot definition
        Expr::func_app(
            SNAP_FUNC_NAME.to_string(),
            vec![expr.clone()],
            vec![vir::LocalVar::new("_", expr.get_type().clone())],
            ty,
            vir::Position::default(),
        )
    }

    /// Converts variant + field access on a snapshot to a domain function call.+
    /// This is used when accessing data of an enum variant.
    pub(super) fn snap_variant_field<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        expr: Expr,
        variant: vir::Field,
        field: vir::Field,
    ) -> EncodingResult<Expr> {
        // TODO: we should not rely on string operations
        assert!(variant.name.starts_with("enum_"));
        let variant_name = &variant.name[5..];
        let snapshot = self.decode_snapshot(encoder, expr.get_type())?;
        match snapshot {
            Snapshot::Complex { variants, variant_names, .. } => {
                let variant_idx = variant_names.get(variant_name)
                    .ok_or_else(|| EncodingError::internal(
                        format!("no such variant: {}", variant_name),
                    ))?;
                variants[*variant_idx].1.get(&field.name)
                    .map(|func| func.apply(vec![expr.clone()]))
                    .ok_or_else(|| EncodingError::internal(format!(
                        "cannot snap_variant_field {}/{} of {:?}",
                        variant_name,
                        field.name,
                        expr,
                    )))
            },
            _ => Err(EncodingError::internal(
                format!("invalid snapshot field (not Complex): {:?}", expr),
            )),
        }
    }

    /// Converts field access on a snapshot to a domain function call.
    pub(super) fn snap_field<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        expr: Expr,
        field: vir::Field,
    ) -> EncodingResult<Expr> {
        let snapshot = self.decode_snapshot(encoder, expr.get_type())?;
        match (field.name.as_str(), snapshot) {
            ("discriminant", Snapshot::Complex { discriminant_func, .. }) => Ok(
                discriminant_func.apply(vec![expr]),
            ),
            (_, Snapshot::Complex { variants, .. }) => variants[0].1
                .get(&field.name)
                .map(|func| func.apply(vec![expr.clone()]))
                .ok_or_else(|| EncodingError::internal(
                    format!("cannot snap_field {} of {:?}", field.name, expr),
                )),
            _ => Err(EncodingError::internal(
                format!("invalid snapshot field (not Complex): {:?}", expr),
            )),
        }
    }

    /// Decodes a VIR type back into a full Snapshot.
    fn decode_snapshot<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        ty: &vir::Type,
    ) -> EncodingResult<Snapshot> {
        match ty {
            Type::Snapshot(ref name)
            | Type::TypedRef(ref name) => {
                let ty = encoder.decode_type_predicate(name)?;
                self.encode_snapshot(encoder, ty)
            }
            _ => Err(EncodingError::internal(
                format!("expected Snapshot type: {:?}", ty),
            )),
        }
    }

    /// Returns a unit domain expression.
    fn snap_unit(&mut self) -> Expr {
        self.unit_used = true;
        self.unit_domain.functions[0].apply(vec![])
    }

    /// Returns [true] iff we can encode equality between two instances of the
    /// given type as a direct equality between snapshots of the instances.
    pub fn supports_equality<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        ty: ty::Ty<'tcx>,
    ) -> EncodingResult<bool> {
        self.encode_snapshot(encoder, ty)
            .map(|snapshot| snapshot.supports_equality())
    }

    /// Returns [true] iff the given type can be used as a quantified variable
    /// in a user-facing [forall].
    pub fn is_quantifiable<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        ty: ty::Ty<'tcx>,
    ) -> EncodingResult<bool> {
        self.encode_snapshot(encoder, ty)
            .map(|snapshot| snapshot.is_quantifiable())
    }

    /// Encodes the postcondition asserting that the discriminant of the
    /// snapshot matches the result of the (Ref-based) discriminant function.
    pub fn encode_discriminant_post<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        expr_self: Expr,
        expr_result: Expr,
    ) -> EncodingResult<Expr> {
        let snapshot = self.decode_snapshot(encoder, expr_self.get_type())?;
        match snapshot {
            Snapshot::Complex { ref discriminant_func, .. } => Ok(vir::Expr::eq_cmp(
                discriminant_func.apply(vec![
                    self.snap_app_expr(expr_self, snapshot.get_type()),
                ]),
                expr_result,
            )),
            _ => Err(EncodingError::internal(
                format!("invalid discriminant post (not Complex): {:?}", expr_self),
            )),
        }
    }

    /// Encodes a snapshot constructor directly. Can only be used on ADTs with
    /// a single variant.
    pub fn encode_constructor<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        ty: ty::Ty<'tcx>,
        args: Vec<vir::Expr>,
    ) -> EncodingResult<vir::Expr> {
        let snapshot = self.encode_snapshot(encoder, ty)?;
        match snapshot {
            Snapshot::Complex { ref variants, .. } => {
                assert_eq!(variants.len(), 1);
                Ok(variants[0].0.apply(args))
            },
            Snapshot::Array { cons, .. } => {
                // the caller must have created a vir::Expr::Seq already
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], vir::Expr::Seq(..)), "Seq expected for array snapshot");

                Ok(cons.apply(args))
            }
            _ => Err(EncodingError::internal(
                format!("invalid constructor (not Complex): {}", ty),
            )),
        }
    }

    /// Encode the array indexing operation `a[i]`. Basically translated into `read(snap(a), i)`.
    pub fn encode_array_idx<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        array_ty: ty::Ty<'tcx>,
        array: vir::Expr,
        idx: vir::Expr,
        ) -> EncodingResult<vir::Expr> {
        let read_func = if let Snapshot::Array { read, .. } = self.encode_snapshot(encoder, array_ty)? {
            read
        } else {
            return Err(EncodingError::internal(
                format!("called encode_array_idx on non-array-type {:?}", array_ty)
            ));
        };

        Ok(read_func.apply(
            vec![
                array,
                idx,
            ],
        ))
    }

    /// Encodes the snapshot of the given type and returns a VIR type
    /// representing that snapshot.
    pub fn encode_type<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        ty: ty::Ty<'tcx>,
    ) -> EncodingResult<Type> {
        self.encode_snapshot(encoder, ty)
            .map(|snapshot| snapshot.get_type())
    }

    /// Starts the snapshot encoding for the given type. This function is
    /// a wrapper that takes care of:
    /// * Resolving the given type and stripping it down to the type we will
    ///   actually encode.
    /// * Caching to avoid duplicate encodings.
    /// * Preventing infinite loops when encoding recursive types.
    /// * Triggering the real encoding [encode_snapshot_internal].
    fn encode_snapshot<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        ty: ty::Ty<'tcx>,
    ) -> EncodingResult<Snapshot> {
        let ty = encoder.resolve_typaram(strip_refs_and_boxes(ty));
        let predicate_name = encoder.encode_type_predicate_use(ty)?;

        // was the snapshot for the type already encoded?
        if let Some(snapshot) = self.encoded.get(&predicate_name) {
            return Ok((*snapshot).clone()); // TODO: can we not clone?
        }

        // is the snapshot currently being encoded? (handle recursive data types)
        if let Some(ty) = self.in_progress.get(&predicate_name) {
            return Ok(Snapshot::Lazy(ty.clone()))
        }

        // otherwise, encode

        // figure out resulting type
        let snapshot_type = match ty.kind() {
            ty::TyKind::Int(_) => Type::Int,
            ty::TyKind::Uint(_) => Type::Int,
            ty::TyKind::Char => Type::Int,
            ty::TyKind::Bool => Type::Bool,
            ty::TyKind::Float(ty::FloatTy::F32) => Type::Float(FloatSize::F32),
            ty::TyKind::Float(ty::FloatTy::F64) => Type::Float(FloatSize::F64),
            ty::TyKind::Tuple(substs) if substs.is_empty() => self.snap_unit().get_type().clone(),
            ty::TyKind::Adt(adt_def, _) if adt_def.variants.is_empty() => self.snap_unit().get_type().clone(),
            ty::TyKind::Adt(adt_def, _) if adt_def.variants.len() == 1 && adt_def.variants[rustc_target::abi::VariantIdx::from_u32(0)].fields.is_empty() => self.snap_unit().get_type().clone(),

            // Param(_) | Adt(_) | Tuple(_) and unsupported types
            _ => Type::Snapshot(predicate_name.to_string()),
        };

        // record in-progress encoding
        self.in_progress.insert(predicate_name.to_string(), snapshot_type.clone());

        // encode snapshot
        let snapshot = self
            .encode_snapshot_internal(encoder, ty, &predicate_name)
            .or_else(|err| {
                self.in_progress.remove(&predicate_name);
                Err(err)
            })?;

        // remove in-progress encoding
        self.in_progress.remove(&predicate_name);

        // remember resulting snapshot
        assert_eq!(snapshot_type, snapshot.get_type());
        self.encoded.insert(predicate_name.to_string(), snapshot.clone());

        Ok(snapshot)
    }

    fn encode_snapshot_internal<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        ty: ty::Ty<'tcx>,
        predicate_name: &str,
    ) -> EncodingResult<Snapshot> {
        let tcx = encoder.env().tcx();

        let arg_self = vir::LocalVar::new(
            "self",
            Type::TypedRef(predicate_name.to_string()),
        );
        let arg_expr = Expr::local(arg_self.clone());

        match ty.kind() {
            // since all encoding goes through [encode_type] first, we should
            // never get a box or reference here
            _ if ty.is_box() => unreachable!(),
            ty::TyKind::Ref(_, _, _) => unreachable!(),

            ty::TyKind::Int(_)
            | ty::TyKind::Uint(_)
            | ty::TyKind::Char => Ok(Snapshot::Primitive(Type::Int)),
            ty::TyKind::Bool => Ok(Snapshot::Primitive(Type::Bool)),
            ty::TyKind::Float(ty::FloatTy::F32) => Ok(Snapshot::Primitive(Type::Float(FloatSize::F32))),
            ty::TyKind::Float(ty::FloatTy::F64) => Ok(Snapshot::Primitive(Type::Float(FloatSize::F64))),

            // handle types with no data
            ty::TyKind::Tuple(substs) if substs.is_empty() => Ok(Snapshot::Unit),
            ty::TyKind::Adt(adt_def, _) if adt_def.variants.is_empty() => Ok(Snapshot::Unit),
            ty::TyKind::Adt(adt_def, _) if adt_def.variants.len() == 1 && adt_def.variants[rustc_target::abi::VariantIdx::from_u32(0)].fields.is_empty() => Ok(Snapshot::Unit),

            // TODO: closures, never type

            ty::TyKind::Tuple(substs) => {
                let mut fields = vec![];
                for (field_num, field_ty) in substs.iter().enumerate() {
                    let field_name = format!("tuple_{}", field_num);
                    let field_ty = field_ty.expect_ty(); // why not use substs?
                    fields.push(SnapshotField {
                        name: field_name.to_string(),
                        access: self.snap_app(encoder, Expr::field(
                            arg_expr.clone(),
                            encoder.encode_raw_ref_field(field_name.to_string(), field_ty)?,
                        ))?,
                        mir_type: field_ty,
                        typ: self.encode_type(encoder, field_ty)?,
                    });
                }
                self.encode_complex(encoder, vec![SnapshotVariant {
                    discriminant: -1,
                    fields,
                    name: None,
                }], predicate_name)
            }
            ty::TyKind::Adt(adt_def, subst) if adt_def.is_struct() => {
                let mut fields = vec![];
                for field in adt_def.all_fields() { // or adt_def.variants[0].fields ?
                    let field_ty = field.ty(tcx, subst);
                    fields.push(SnapshotField {
                        name: format!("f${}", field.ident),
                        access: self.snap_app(encoder, Expr::field(
                            arg_expr.clone(),
                            encoder.encode_struct_field(&field.ident.to_string(), field_ty)?,
                        ))?,
                        mir_type: field_ty,
                        typ: self.encode_type(encoder, field_ty)?,
                    });
                }
                self.encode_complex(encoder, vec![SnapshotVariant {
                    discriminant: -1,
                    fields,
                    name: None,
                }], predicate_name)
            }
            ty::TyKind::Adt(adt_def, subst) if adt_def.is_enum() => {
                let mut variants = vec![];
                let predicate = encoder.encode_type_predicate_def(ty)?;
                for (variant_idx, variant) in adt_def.variants.iter_enumerated() {
                    let mut fields = vec![];
                    let variant_idx: usize = variant_idx.into();
                    let (field_base, variant_name) = match predicate {
                        vir::Predicate::Enum(ref enum_predicate) => {
                            let (_, ref variant_name, _) = enum_predicate.variants[variant_idx];
                            (arg_expr.clone().variant(variant_name), Some(variant_name.to_string()))
                        }
                        vir::Predicate::Struct(..) => {
                            (arg_expr.clone(), None)
                        }
                        _ => return Err(EncodingError::internal(
                            format!("invalid Predicate for ADT: {}", predicate),
                        )),
                    };
                    for field in &variant.fields {
                        let field_ty = field.ty(tcx, subst);
                        fields.push(SnapshotField {
                            name: format!("f${}", field.ident),
                            access: self.snap_app(encoder, Expr::field(
                                field_base.clone(),
                                encoder.encode_struct_field(&field.ident.to_string(), field_ty)?,
                            ))?,
                            mir_type: field_ty,
                            typ: self.encode_type(encoder, field_ty)?,
                        });
                    }

                    let discriminant_raw = adt_def.discriminant_for_variant(
                        tcx,
                        rustc_target::abi::VariantIdx::from_usize(variant_idx)
                    ).val;
                    let size = ty::tls::with(|tcx| Integer::from_attr(&tcx, adt_def.repr.discr_type()).size());
                    let discriminant = size.sign_extend(discriminant_raw) as i128;
                    variants.push(SnapshotVariant {
                        discriminant,
                        fields,
                        name: variant_name,
                    });
                }
                self.encode_complex(encoder, variants, predicate_name)
            }

            ty::TyKind::Array(elem_ty, ..) => {
                let elem_snap_ty = self.encode_type(encoder, elem_ty)?;
                let array_types = encoder.encode_array_types(ty)?;

                let domain_name = format!("Snap${}", &array_types.array_pred);
                let snap_type = Type::Snapshot(array_types.array_pred.clone());
                let seq_type = Type::Seq(box elem_snap_ty.clone());

                let cons = vir::DomainFunc {
                    name: format!("cons${}$", domain_name),
                    formal_args: vec![
                        vir_local!{ data: {seq_type.clone()} },
                    ],
                    return_type: snap_type.clone(),
                    unique: false,
                    domain_name: domain_name.clone(),
                };

                let snap_body = cons.apply(
                    vec![
                        Expr::Seq(
                            Type::Seq(box elem_snap_ty.clone()),
                            (0..array_types.array_len)
                                .into_iter()
                                .map(|idx| {
                                    array_types.encode_lookup_pure_call(
                                        encoder,
                                        arg_expr.clone(),
                                        idx.into(),
                                        elem_snap_ty.clone(),
                                    )
                                })
                                .collect(),
                            vir::Position::default(),
                        )
                    ]
                );

                let read = vir::DomainFunc {
                    name: format!("read${}$", domain_name),
                    formal_args: vec![
                        vir_local!{ arr: {snap_type.clone()} },
                        vir_local!{ idx: Int },
                    ],
                    return_type: elem_snap_ty.clone(),
                    unique: false,
                    domain_name: domain_name.clone(),
                };

                let i: vir::Expr = vir_local!{ i: Int }.into();
                let lookup_call = array_types.encode_lookup_pure_call(
                    encoder,
                    arg_expr.clone(),
                    i.clone(),
                    elem_snap_ty,
                );
                let read_call = read.apply(vec![
                        vir_local!{ __result: {snap_type.clone()} }.into(),
                        i.clone()
                ]);

                let indices = vir!{ ([vir::Expr::from(0)] <= [i]) && ([i] < [vir::Expr::from(array_types.array_len)]) };

                let read_eq_lookup = vir!{ forall i: Int :: { [read_call], [lookup_call] } ([indices] ==> ([read_call] == [lookup_call])) };

                let snap_func = vir::Function {
                    name: SNAP_FUNC_NAME.to_string(),
                    formal_args: vec![arg_self],
                    return_type: snap_type,
                    pres: vec![
                        Expr::predicate_access_predicate(
                            array_types.array_pred,
                            arg_expr,
                            PermAmount::Read,
                        ),
                    ],
                    posts: vec![
                        // FIXME: this shouldn't be necessary, would want to just use
                        // read_eq_lookup here, but doesn't verify always (for big arrays
                        // especially)
                        vir::Expr::InhaleExhale(box read_eq_lookup, box true.into(), vir::Position::default()),
                    ],
                    body: Some(snap_body),
                };

                let constructor_inj = {
                    let lhs_arg = vir_local!{ _l_data: {seq_type.clone()} };
                    let rhs_arg = vir_local!{ _r_data: {seq_type.clone()} };

                    let lhs_call = cons.apply(vec![lhs_arg.clone().into()]);
                    let rhs_call = cons.apply(vec![rhs_arg.clone().into()]);

                    vir::DomainAxiom {
                        name: format!("{}$injectivity", domain_name),
                        expr: Expr::forall(
                            vec![lhs_arg.clone(), rhs_arg.clone()],
                            vec![vir::Trigger::new(vec![lhs_call.clone(), rhs_call.clone()])],
                            Expr::implies(
                                vir!{ [lhs_call] == [rhs_call] },
                                vir!{ [vir::Expr::from(lhs_arg)] == [vir::Expr::from(rhs_arg)] },
                            )
                        ),
                        domain_name: domain_name.clone(),
                    }
                };

                let read_axiom = {
                    let data = vir_local!{ data: {seq_type} };
                    let idx = vir_local!{ idx: Int };

                    let cons_call = cons.apply(vec![data.clone().into()]);
                    let read_call = read.apply(vec![cons_call, idx.clone().into()]);

                    let seq_lookup = Expr::ContainerOp(
                        ContainerOpKind::SeqIndex,
                        box data.clone().into(),
                        box idx.clone().into(),
                        vir::Position::default(),
                    );

                    vir::DomainAxiom {
                        name: format!("{}$read_indices", predicate_name),
                        expr: Expr::forall(
                            vec![
                                data,
                                idx,
                            ],
                            vec![
                                vir::Trigger::new(vec![read_call.clone()]),
                                vir::Trigger::new(vec![seq_lookup.clone()]),
                            ],
                            vir!{ [read_call] == [seq_lookup] },
                        ),
                        domain_name: domain_name.clone(),
                    }
                };

                let domain = vir::Domain {
                    name: domain_name,
                    functions: vec![
                        cons.clone(),
                        read.clone(),
                    ],
                    axioms: vec![
                        constructor_inj,
                        read_axiom,
                    ],
                    type_vars: vec![],
                };

                Ok(Snapshot::Array {
                    predicate_name: predicate_name.to_string(),
                    domain,
                    snap_func,
                    cons,
                    read,
                })
            }

            // Param(_) and unsupported types
            _ => self.encode_abstract(predicate_name),
        }
    }

    fn encode_abstract<'p, 'v: 'p, 'tcx: 'v>(
        &self,
        predicate_name: &str,
    ) -> EncodingResult<Snapshot> {
        let domain_name = format!("Snap${}", predicate_name);
        let snapshot_type = Type::Snapshot(predicate_name.to_string());

        let arg_ref_local = vir::LocalVar::new(
            "self",
            Type::TypedRef(predicate_name.to_string()),
        );
        let arg_ref_expr = Expr::local(arg_ref_local.clone());

        // encode snap function
        let snap_func = vir::Function {
            name: SNAP_FUNC_NAME.to_string(),
            formal_args: vec![arg_ref_local.clone()],
            return_type: snapshot_type.clone(),
            pres: vec![Expr::predicate_access_predicate(
                predicate_name,
                arg_ref_expr.clone(),
                PermAmount::Read,
            )],
            posts: vec![],
            body: None,
        };

        Ok(Snapshot::Abstract {
            predicate_name: predicate_name.to_string(),
            domain: vir::Domain {
                name: domain_name,
                functions: vec![],
                axioms: vec![],
                type_vars: vec![],
            },
            snap_func,
        })
    }

    /// Encodes the snapshot for a complex data structure (tuple, struct,
    /// enum, or closure). There must be one or more variants, at least one
    /// with one or more fields to encode. The returned snapshot will be of the
    /// [Snapshot::Complex] variant.
    fn encode_complex<'p, 'v: 'p, 'tcx: 'v>(
        &self,
        encoder: &'p Encoder<'v, 'tcx>,
        variants: Vec<SnapshotVariant<'tcx>>,
        predicate_name: &str,
    ) -> EncodingResult<Snapshot> {
        let domain_name = format!("Snap${}", predicate_name);
        let snapshot_type = Type::Snapshot(predicate_name.to_string());
        let has_multiple_variants = variants.len() > 1;
        let mut domain_funcs = vec![];
        let mut domain_axioms = vec![];
        let mut variant_domain_funcs = vec![];
        let mut variant_snap_bodies = vec![];
        let mut variant_names = HashMap::new();

        // a local called "self", both as a Ref and as a Snapshot
        let arg_ref_local = vir::LocalVar::new(
            "self",
            Type::TypedRef(predicate_name.to_string()),
        );
        let arg_dom_local = vir::LocalVar::new(
            "self",
            snapshot_type.clone(),
        );
        let arg_ref_expr = Expr::local(arg_ref_local.clone());
        let arg_dom_expr = Expr::local(arg_dom_local.clone());

        // encode discriminant function
        let discriminant_func = vir::DomainFunc {
            name: "discriminant$".to_string(),
            formal_args: vec![arg_dom_local.clone()],
            return_type: Type::Int,
            unique: false,
            domain_name: domain_name.to_string(),
        };

        if has_multiple_variants {
            domain_funcs.push(discriminant_func.clone());

            // encode discriminant range axiom
            domain_axioms.push({
                let disc_call = discriminant_func.apply(vec![arg_dom_expr.clone()]);
                vir::DomainAxiom {
                    name: format!("{}$discriminant_range", domain_name),
                    expr: Expr::forall(
                        vec![arg_dom_local.clone()],
                        vec![vir::Trigger::new(vec![disc_call.clone()])],
                        range_extract(
                            variants
                                .iter()
                                .map(|v| v.discriminant)
                                .collect()
                        )
                            .into_iter()
                            .map(|(from, to)| {
                                if from == to {
                                    Expr::eq_cmp(disc_call.clone(), from.into())
                                } else {
                                    Expr::and(
                                        Expr::le_cmp(from.into(), disc_call.clone()),
                                        Expr::le_cmp(disc_call.clone(), to.into()),
                                    )
                                }
                            })
                            .disjoin(),
                    ),
                    domain_name: domain_name.to_string(),
                }
            });
        }

        // for each variant, we need to encode.
        // * the constructor, which takes the flattened value-only
        //   representation of the variant and returns an instance of the
        //   snapshot domain
        // * the injectivity axiom for that constructor:
        //   ```plain
        //   forall _l_args..., _r_args... :: {cons(_l_args...), cons(_r_args)}
        //     cons(_l_args...) == cons(_r_args) ==> _l_args... == _r_args...
        //   ```
        // * the discriminant axiom for that constructor:
        //   ```plain
        //   discriminant(cons<N>(...)) == N
        //   ```
        // * for each field in the variant:
        //   * field access function
        //   * field access axiom:
        //     ```plain
        //     forall args... :: {field(cons(arg_field, other_args...))}
        //       field(cons(arg_field, other_args...)) == arg_field
        //     ```
        for (variant_idx, variant) in variants.iter().enumerate() {
            let constructor_name = format!("cons${}$", variant_idx);
            let args = variant.fields.iter()
                .enumerate()
                .map(|(idx, field)| vir::LocalVar::new(
                    format!("_{}", idx),
                    field.typ.clone(),
                )).collect::<Vec<vir::LocalVar>>();
            // TODO: filter out Units to reduce constructor size
            let has_args = !args.is_empty();

            // record name to index mapping
            if let Some(name) = &variant.name {
                variant_names.insert(name.to_string(), variant_idx);
            }

            // encode constructor
            let constructor = vir::DomainFunc {
                name: constructor_name.to_string(),
                formal_args: args.clone(),
                return_type: snapshot_type.clone(),
                unique: false,
                domain_name: domain_name.to_string(),
            };
            domain_funcs.push(constructor.clone());

            let encode_prefixed_args = |prefix: &str| -> Vec<vir::LocalVar> {
                args.iter().map(|arg| vir::LocalVar::new(
                        format!("{}{}", prefix, arg.name),
                        arg.typ.clone(),
                    )).collect()
            };
            let encode_constructor_call = |args: &Vec<vir::LocalVar>| -> Expr {
                constructor.apply(args.iter().cloned().map(Expr::local).collect())
            };

            if has_args {
                // encode injectivity axiom of constructor
                let lhs_args = encode_prefixed_args("_l");
                let rhs_args = encode_prefixed_args("_r");

                let lhs_call = encode_constructor_call(&lhs_args);
                let rhs_call = encode_constructor_call(&rhs_args);

                let mut forall_vars = vec![];
                forall_vars.extend(lhs_args.iter().cloned());
                forall_vars.extend(rhs_args.iter().cloned());

                let conjunction = lhs_args.iter().cloned()
                    .zip(rhs_args.iter().cloned())
                    .map(|(l, r)| Expr::eq_cmp(Expr::local(l), Expr::local(r)))
                    .conjoin();

                domain_axioms.push(vir::DomainAxiom {
                    name: format!("{}${}$injectivity", domain_name, variant_idx),
                    expr: forall_or_body(
                        forall_vars,
                        vec![vir::Trigger::new(vec![lhs_call.clone(), rhs_call.clone()])],
                        Expr::implies(
                            Expr::eq_cmp(lhs_call, rhs_call),
                            conjunction,
                        ),
                    ),
                    domain_name: domain_name.to_string(),
                });
            }

            if has_multiple_variants {
                // encode discriminant axiom
                domain_axioms.push({
                    let args = encode_prefixed_args("");
                    let call = encode_constructor_call(&args);
                    vir::DomainAxiom {
                        name: format!("{}${}$discriminant_axiom", domain_name, variant_idx),
                        expr: forall_or_body(
                            args.iter().cloned().collect(),
                            vec![vir::Trigger::new(vec![
                                call.clone(),
                            ])],
                            Expr::eq_cmp(
                                discriminant_func.apply(vec![call.clone()]),
                                variant.discriminant.into(),
                            ),
                        ),
                        domain_name: domain_name.to_string(),
                    }
                });
            }

            let mut field_access_funcs = HashMap::new();

            for (field_idx, field) in variant.fields.iter().enumerate() {
                // encode field access function
                let field_access_func = vir::DomainFunc {
                    name: format!("{}${}$field${}", domain_name, variant_idx, field.name),
                    formal_args: vec![vir::LocalVar::new(
                        "self",
                        snapshot_type.clone(),
                    )],
                    return_type: field.typ.clone(),
                    unique: false,
                    domain_name: domain_name.to_string(),
                };
                domain_funcs.push(field_access_func.clone());
                field_access_funcs.insert(field.name.to_string(), field_access_func.clone());

                // encode field access axiom
                domain_axioms.push({
                    let args = encode_prefixed_args("");
                    let call = encode_constructor_call(&args);
                    let field_of_cons = field_access_func.apply(vec![call.clone()]);

                    vir::DomainAxiom {
                        name: format!("{}${}$field${}$axiom", domain_name, variant_idx, field.name),
                        expr: forall_or_body(
                            args.clone(),
                            vec![vir::Trigger::new(vec![
                                field_of_cons.clone(),
                            ])],
                            Expr::eq_cmp(
                                field_of_cons.clone(),
                                Expr::local(args[field_idx].clone()),
                            ),
                        ),
                        domain_name: domain_name.to_string(),
                    }
                });

                // encode type validity axiom for field
                // TODO: encode type invariants rather than just integer bounds
                match field.mir_type.kind() {
                    ty::TyKind::Int(_)
                    | ty::TyKind::Uint(_)
                    | ty::TyKind::Char => domain_axioms.push({
                        let self_local = vir::LocalVar::new(
                            "self",
                            snapshot_type.clone(),
                        );
                        let self_expr = Expr::local(self_local.clone());
                        let field_of_self = field_access_func.apply(vec![self_expr.clone()]);

                        vir::DomainAxiom {
                            name: format!("{}${}$field${}$valid", domain_name, variant_idx, field.name),
                            expr: Expr::forall(
                                vec![self_local.clone()],
                                vec![vir::Trigger::new(vec![
                                    field_of_self.clone(),
                                ])],
                                encoder.encode_type_bounds(&field_of_self, field.mir_type)
                                    .into_iter()
                                    .conjoin(),
                            ),
                            domain_name: domain_name.to_string(),
                        }
                    }),
                    _ => {},
                }
            }

            variant_domain_funcs.push((constructor.clone(), field_access_funcs));

            // encode constructor call for this variant
            variant_snap_bodies.push(constructor.apply(
                variant.fields.iter()
                    .map(|f| f.access.clone())
                    .collect()
            ));
        }

        // encode snap function
        let snap_func = {
            // <discriminant> == <discriminant of variant 0>
            //   ? cons0(fields_of_variant_0...)
            //   : ...

            // TODO: avoid clone somehow?
            let mut body = variant_snap_bodies[0].clone();
            for i in 1..variants.len() {
                body = Expr::ite(
                    Expr::eq_cmp(
                        Expr::field(
                            arg_ref_expr.clone(),
                            encoder.encode_discriminant_field(),
                        ),
                        variants[i].discriminant.into(),
                    ),
                    variant_snap_bodies[i].clone(),
                    body,
                );
            }

            vir::Function {
                name: SNAP_FUNC_NAME.to_string(),
                formal_args: vec![arg_ref_local.clone()],
                return_type: snapshot_type.clone(),
                pres: vec![Expr::predicate_access_predicate(
                    predicate_name,
                    arg_ref_expr.clone(),
                    PermAmount::Read,
                )],
                posts: vec![],
                body: Some(body),
            }
        };
        let snap_func = foldunfold::add_folding_unfolding_to_function(
            snap_func,
            encoder.get_used_viper_predicates_map(),
        ).unwrap();

        // create domain
        let domain = vir::Domain {
            name: domain_name,
            functions: domain_funcs,
            axioms: domain_axioms,
            type_vars: vec![],
        };

        Ok(Snapshot::Complex {
            predicate_name: predicate_name.to_string(),
            domain,
            discriminant_func,
            snap_func,
            variants: variant_domain_funcs,
            variant_names,
        })
    }
}

struct SnapshotVariant<'tcx> {
    discriminant: i128, // FIXME: Option<i128>, for now -1 when not applicable
    fields: Vec<SnapshotField<'tcx>>,
    name: Option<String>,
}

struct SnapshotField<'tcx> {
    name: String,
    access: Expr, // _ARG(.field)*
    mir_type: ty::Ty<'tcx>,
    typ: Type,
}
