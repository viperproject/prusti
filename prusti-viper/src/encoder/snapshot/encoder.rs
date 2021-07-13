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
    array_encoder::{EncodedArrayTypes, EncodedSliceTypes},
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
                | Snapshot::Abstract { snap_func, .. } => funcs.push(snap_func.clone()),
                Snapshot::Array { snap_func, slice_helper, .. } => {
                    funcs.extend([snap_func.clone(), slice_helper.clone()]);
                }
                Snapshot::Slice { snap_func, slice_collect_func, slice_helper, .. } => {
                    funcs.extend([snap_func.clone(), slice_collect_func.clone(), slice_helper.clone()]);
                }
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
                | Snapshot::Array { domain, .. }
                | Snapshot::Slice { domain, .. } => domains.push(domain.clone()),
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
                        vir::Field::new("val_float32", Type::Bool),
                    ),
                    ty::TyKind::Float(ty::FloatTy::F64) => Expr::field(
                        expr.clone(),
                        vir::Field::new("val_float64", Type::Bool),
                    ),
                    ty::TyKind::Tuple(substs) if substs.is_empty() => self.snap_unit(),
                    ty::TyKind::Adt(adt_def, _) if adt_def.variants.is_empty() => self.snap_unit(),
                    ty::TyKind::Adt(adt_def, _) if adt_def.variants.len() == 1 && adt_def.variants[rustc_target::abi::VariantIdx::from_u32(0)].fields.is_empty() => self.snap_unit(),

                    // Param(_) | Adt(_) | Tuple(_), arrays and slices and unsupported types
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
            Snapshot::Slice { cons, .. } => {
                // the caller must have created a vir::Expr::Seq already
                assert_eq!(args.len(), 1);
                // args[0] is the `Seq` of elements
                assert!(matches!(args[0], vir::Expr::Seq(..)), "Seq expected for slice snapshot");

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

    /// Encode slice indexing operation `a[i]`.
    pub fn encode_slice_idx<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        slice_ty: ty::Ty<'tcx>,
        slice: vir::Expr,
        idx: vir::Expr,
    ) -> EncodingResult<vir::Expr> {
        let read_func = if let Snapshot::Slice { read, .. } = self.encode_snapshot(encoder, slice_ty)? {
            read
        } else {
            return Err(EncodingError::internal(
                format!("called encode_slice_idx on non-slice-type {:?}", slice_ty)
            ));
        };

        Ok(read_func.apply(
            vec![
                slice,
                idx,
            ],
        ))
    }

    pub fn encode_slice_len<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        slice_ty: ty::Ty<'tcx>,
        slice: vir::Expr,
    ) -> EncodingResult<vir::Expr> {
        let len_func = if let Snapshot::Slice { len, .. } = self.encode_snapshot(encoder, slice_ty)? {
            len
        } else {
            return Err(EncodingError::internal(
                format!("called encode_slice_len on non-slice-type {:?}", slice_ty)
            ));
        };

        Ok(len_func.apply(vec![slice]))
    }

    pub fn encode_slicing<'p, 'v: 'p, 'tcx: 'v>(
        &mut self,
        encoder: &'p Encoder<'v, 'tcx>,
        base_ty: ty::Ty<'tcx>,
        base: vir::Expr,
        slice_ty: ty::Ty<'tcx>,
        lo: vir::Expr,
        hi: vir::Expr,
    ) -> EncodingResult<vir::Expr> {
        match self.encode_snapshot(encoder, base_ty)? {
            Snapshot::Array { slice_helper, .. }
            | Snapshot::Slice { slice_helper, .. } => {
                let slice_cons = if let Snapshot::Slice { cons, .. } = self.encode_snapshot(encoder, slice_ty)? {
                    cons
                } else {
                    return Err(EncodingError::internal(
                        format!("called encode_slicing on non-slice-type {:?}", slice_ty)
                    ));
                };

                Ok(slice_cons.apply(vec![slice_helper.apply(vec![base, lo, hi])]))
            }
            _ => Err(EncodingError::internal(
                format!("cannot slice type {:?}", base_ty)
            )),
        }
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

            // Param(_) | Adt(_) | Tuple(_), arrays and slices and unsupported types
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
                            seq_type.clone(),
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

                let read_eq_lookup = {
                    let i: vir::Expr = vir_local!{ i: Int }.into();
                    let lookup_call = array_types.encode_lookup_pure_call(
                        encoder,
                        arg_expr.clone(),
                        i.clone(),
                        elem_snap_ty.clone(),
                    );
                    let read_call = read.apply(vec![
                            vir_local!{ __result: {snap_type.clone()} }.into(),
                            i.clone()
                    ]);

                    let indices = vir!{ ([vir::Expr::from(0)] <= [i]) && ([i] < [vir::Expr::from(array_types.array_len)]) };

                    vir!{ forall i: Int :: { [read_call], [lookup_call] } ([indices] ==> ([read_call] == [lookup_call])) }
                };

                let snap_func = vir::Function {
                    name: SNAP_FUNC_NAME.to_string(),
                    formal_args: vec![arg_self],
                    return_type: snap_type.clone(),
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

                let slice_helper = self.encode_slice_helper(
                    snap_type,
                    elem_snap_ty,
                    read.clone(),
                    array_types.array_len.into(),
                );

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
                    slice_helper,
                    cons,
                    read,
                })
            }

            ty::TyKind::Slice(elem_ty) => {
                let slice_types = encoder.encode_slice_types(ty)?;
                let domain_name = format!("Snap${}", &slice_types.slice_pred);
                let slice_snap_ty = Type::Snapshot(slice_types.slice_pred.clone());
                let elem_snap_ty = self.encode_type(encoder, elem_ty)?;
                let seq_type = Type::Seq(box elem_snap_ty.clone());

                let cons = vir::DomainFunc {
                    name: format!("cons${}$", domain_name),
                    formal_args: vec![
                        vir_local!{ data: {seq_type.clone()} },
                    ],
                    return_type: slice_snap_ty.clone(),
                    unique: false,
                    domain_name: domain_name.clone(),
                };

                let len = vir::DomainFunc {
                    name: format!("len${}$", domain_name),
                    formal_args: vec![
                        vir_local!{ self: {slice_snap_ty.clone()} },
                    ],
                    return_type: vir::Type::Int,
                    unique: false,
                    domain_name: domain_name.clone(),
                };

                let read = vir::DomainFunc {
                    name: format!("read${}$", domain_name),
                    formal_args: vec![
                        vir_local!{ self: {slice_snap_ty.clone()} },
                        vir_local!{ idx: Int },
                    ],
                    return_type: elem_snap_ty.clone(),
                    unique: false,
                    domain_name: domain_name.clone(),
                };

                let slice_collect_funcname = format!("slice_collect${}$", slice_types.slice_pred);

                let start = vir_local!{ start: Int };
                let start_expr: vir::Expr = start.clone().into();
                let result_expr: vir::Expr = vir_local!{ __result: {seq_type.clone()} }.into();
                let i = vir_local!{ i: Int };
                let i_expr: vir::Expr = i.clone().into();
                let j = vir_local!{ j: Int };
                let j_expr: vir::Expr = j.clone().into();

                let slice_len_call = slice_types.encode_slice_len_call(encoder, arg_expr.clone());
                let empty_seq = Expr::Seq(seq_type.clone(), vec![], vir::Position::default());
                let start_lt_len = vir!{ [start_expr] < [slice_len_call] };
                let result_len = Expr::ContainerOp(ContainerOpKind::SeqLen, box result_expr.clone(), box Expr::from(0), vir::Position::default());
                let result_0 = Expr::ContainerOp(ContainerOpKind::SeqIndex, box result_expr.clone(), box Expr::from(0), vir::Position::default());
                let result_j = Expr::ContainerOp(ContainerOpKind::SeqIndex, box result_expr.clone(), box j_expr.clone(), vir::Position::default());
                let slice_lookup_i = slice_types.encode_lookup_pure_call(encoder, arg_expr.clone(), i_expr.clone(), elem_snap_ty.clone());
                let slice_lookup_start = slice_types.encode_lookup_pure_call(encoder, arg_expr.clone(), start_expr.clone(), elem_snap_ty.clone());

                // forall i: Int, j: Int :: { slice$lookup_pure(slice, i), result[j] } idx <= i && i < slice$len(slice) && 0 <= j && j < slice$len(slice)-idx && i == j + idx ==> slice$lookup_pure(slice, i) == result[j]
                let rest_eq_lookup = {
                    let indices = vir!{ (([start_expr] <= [i_expr]) && ([i_expr] < [slice_len_call]))
                        && ((([Expr::from(0)] <= [j_expr]) && ([j_expr] < ([slice_len_call] - [start_expr])))
                        && ([i_expr] == ([j_expr] + [start_expr]))) };

                    vir::Expr::forall(
                        vec![i, j],
                        vec![
                            vir::Trigger::new(vec![slice_lookup_i.clone(), result_j.clone()])
                        ],
                        vir!{ [indices] ==> ([slice_lookup_i] == [result_j]) },
                    )
                };

                let slice_collect_func = vir::Function {
                    name: slice_collect_funcname.clone(),
                    formal_args: vec![
                        vir_local!{ self: {slice_types.slice_ty.clone()} },
                        start.clone(),
                    ],
                    return_type: seq_type.clone(),
                    pres: vec![
                        Expr::predicate_access_predicate(
                            slice_types.slice_pred.clone(),
                            arg_expr.clone(),
                            PermAmount::Read,
                        ),
                        vir!{ [vir::Expr::from(0)] <= [start_expr] },
                    ],
                    posts: vec![
                        vir!{ ([start_expr] >= [slice_len_call]) ==> ([result_expr] == [empty_seq]) },
                        vir!{ [start_lt_len] ==> ([result_len] == ([slice_len_call] - [start_expr])) },
                        vir!{ [start_lt_len] ==> ([result_0] == [slice_lookup_start]) },
                        vir!{ [start_lt_len] ==> [rest_eq_lookup] },
                    ],
                    body: Some(Expr::ite(
                        vir!{ [start_expr] >= [slice_len_call] },
                        Expr::Seq(seq_type.clone(), vec![], vir::Position::default()),
                        Expr::ContainerOp(
                            ContainerOpKind::SeqConcat,
                            box Expr::Seq(
                                seq_type.clone(),
                                vec![
                                    slice_types.encode_lookup_pure_call(encoder, arg_expr.clone(), start_expr.clone(), elem_snap_ty.clone())
                                ],
                                vir::Position::default(),
                            ),
                            box Expr::func_app(
                                slice_collect_funcname,
                                vec![
                                    arg_expr.clone(),
                                    vir!{ [start_expr] + [Expr::from(1)] },
                                ],
                                vec![
                                    vir_local!{ slice: {slice_types.slice_ty.clone()} },
                                    start,
                                ],
                                seq_type.clone(),
                                vir::Position::default(),
                            ),
                            vir::Position::default(),
                        )
                    )),
                };

                let slice_collect_func_app = slice_collect_func.apply(
                    vec![
                        arg_expr.clone(),
                        0.into(),
                    ],
                );

                let snap_body = cons.apply(vec![slice_collect_func_app]);

                let slice_len = slice_types.encode_slice_len_call(
                    encoder,
                    arg_expr.clone(),
                );

                let result_expr: vir::Expr = vir_local!{ __result: {slice_snap_ty.clone()} }.into();

                // TODO: de-duplicate with array case?
                let read_eq_lookup = {
                    let i: vir::Expr = vir_local!{ i: Int }.into();
                    let lookup_call = slice_types.encode_lookup_pure_call(
                        encoder,
                        arg_expr.clone(),
                        i.clone(),
                        elem_snap_ty.clone(),
                    );
                    let read_call = read.apply(vec![
                        result_expr.clone(),
                        i.clone(),
                    ]);

                    let indices = vir!{ ([vir::Expr::from(0)] <= [i]) && ([i] < [slice_len]) };

                    vir!{ forall i: Int :: { [read_call], [lookup_call] } ([indices] ==> ([read_call] == [lookup_call])) }
                };

                let snap_len = len.apply(vec![result_expr]);
                let snap_len_eq_call_len = vir!{ [slice_len] == [snap_len] };

                let snap_func = vir::Function {
                    name: SNAP_FUNC_NAME.to_string(),
                    formal_args: vec![arg_self],
                    return_type: slice_snap_ty.clone(),
                    pres: vec![
                        Expr::predicate_access_predicate(
                            slice_types.slice_pred,
                            arg_expr,
                            PermAmount::Read,
                        ),
                    ],
                    posts: vec![
                        vir::Expr::InhaleExhale(box read_eq_lookup, box true.into(), vir::Position::default()),
                        vir::Expr::InhaleExhale(box snap_len_eq_call_len, box true.into(), vir::Position::default()),
                    ],
                    body: Some(snap_body),
                };

                let slice_helper = self.encode_slice_helper(
                    slice_snap_ty.clone(),
                    elem_snap_ty,
                    read.clone(),
                    len.apply(vec![vir_local!{ self: {slice_snap_ty.clone()} }.into()]),
                );

                let cons_inj = {
                    let data_l = vir_local!{ _l_data: {seq_type.clone()} };
                    let data_r = vir_local!{ _r_data: {seq_type.clone()} };

                    let cons_l = cons.apply(vec![data_l.clone().into()]);
                    let cons_r = cons.apply(vec![data_r.clone().into()]);

                    vir::DomainAxiom {
                        name: format!("{}$injectivity", domain_name),
                        expr: Expr::forall(
                            vec![
                                data_l.clone(),
                                data_r.clone(),
                            ],
                            vec![
                                vir::Trigger::new(vec![cons_l.clone(), cons_r.clone()]),
                            ],
                            Expr::implies(
                                vir!{ [cons_l] == [cons_r] },
                                vir!{ ([vir::Expr::from(data_l)] == [vir::Expr::from(data_r)]) },
                            ),
                        ),
                        domain_name: domain_name.clone(),
                    }
                };

                let data = vir_local!{ data: {seq_type.clone()} };
                let cons_call = cons.apply(vec![data.clone().into()]);

                let read_axiom = {
                    let idx = vir_local!{ idx: Int };

                    let read_call = read.apply(vec![cons_call.clone(), idx.clone().into()]);

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
                                data.clone(),
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

                let len_of_seq = {
                    let len_call = len.apply(vec![cons_call]);
                    let seq_len = Expr::ContainerOp(
                        ContainerOpKind::SeqLen,
                        box data.into(),
                        box true.into(), // unused
                        vir::Position::default(),
                    );

                    vir::DomainAxiom {
                        name: format!("{}$len_of_seq", predicate_name),
                        expr: vir!{ forall data: {seq_type} :: { [len_call], [seq_len] } ([len_call] == [seq_len]) },
                        domain_name: domain_name.clone(),
                    }
                };

                let len_positive = {
                    let len_call = len.apply(vec![vir_local!{ slice: {slice_snap_ty.clone()} }.into()]);

                    vir::DomainAxiom {
                        name: format!("{}$len_positive", predicate_name),
                        expr: vir!{ forall slice: {slice_snap_ty} :: { [len_call] } ([len_call] >= [Expr::from(0)]) },
                        domain_name: domain_name.clone(),
                    }
                };

                let domain = vir::Domain {
                    name: domain_name,
                    functions: vec![
                        cons.clone(),
                        read.clone(),
                        len.clone(),
                    ],
                    axioms: vec![
                        cons_inj,
                        read_axiom,
                        len_of_seq,
                        len_positive,
                    ],
                    type_vars: vec![],
                };

                Ok(Snapshot::Slice {
                    predicate_name: predicate_name.to_string(),
                    domain,
                    snap_func,
                    slice_collect_func,
                    slice_helper,
                    cons,
                    read,
                    len,
                })
            }

            // Param(_) and unsupported types
            _ => self.encode_abstract(predicate_name),
        }
    }

    fn encode_slice_helper(
        &self,
        self_snap_ty: vir::Type,
        elem_snap_ty: vir::Type,
        read: vir::DomainFunc,
        base_len: vir::Expr,
    ) -> vir::Function {
        let slice_helper_name = "slice_helper".to_string();

        let self_expr: vir::Expr = vir_local!{ self: {self_snap_ty.clone()} }.into();
        let seq_type = vir::Type::Seq(box elem_snap_ty.clone());

        let lo = vir_local!{ lo: Int };
        let lo_expr: vir::Expr = lo.clone().into();
        let hi = vir_local!{ hi: Int };
        let hi_expr: vir::Expr = hi.clone().into();

        let result_expr: vir::Expr = vir_local!{ __result: {seq_type.clone()} }.into();
        let i = vir_local!{ i: Int };
        let i_expr: vir::Expr = i.clone().into();
        let j = vir_local!{ j: Int };
        let j_expr: vir::Expr = j.clone().into();

        let result_len = Expr::ContainerOp(ContainerOpKind::SeqLen, box result_expr.clone(), box Expr::from(0), vir::Position::default());
        let result_0 = Expr::ContainerOp(ContainerOpKind::SeqIndex, box result_expr.clone(), box Expr::from(0), vir::Position::default());
        let result_j = Expr::ContainerOp(ContainerOpKind::SeqIndex, box result_expr.clone(), box j_expr.clone(), vir::Position::default());

        let read_i = read.apply(vec![self_expr.clone(), i_expr.clone()]);
        let read_lo = read.apply(vec![self_expr.clone(), lo_expr.clone()]);

        let read_eq_seq_lookup = {
            let indices = vir!{ (([lo_expr] <= [i_expr]) && ([i_expr] < [hi_expr]))
                && ((([Expr::from(0)] <= [j_expr]) && ([j_expr] < [hi_expr]))
                && ([i_expr] == ([j_expr] + [lo_expr]))) };

            vir::Expr::forall(
                vec![i, j],
                vec![
                    vir::Trigger::new(vec![read_i.clone(), result_j.clone()])
                ],
                vir!{ [indices] ==> ([read_i] == [result_j]) },
            )
        };

        vir::Function {
            name: slice_helper_name.clone(),
            formal_args: vec![
                vir_local!{ self: {self_snap_ty.clone()} },
                lo.clone(),
                hi.clone(),
            ],
            return_type: seq_type.clone(),
            pres: vec![
                vir!{ [vir::Expr::from(0)] <= [lo_expr] },
                vir!{ [lo_expr] <= [hi_expr] },
                vir!{ [hi_expr] <= [base_len] },
            ],
            posts: vec![
                vir!{ [result_len] == ([hi_expr] - [lo_expr]) },
                vir!{ ([lo_expr] < [hi_expr]) ==> ([result_0] == [read_lo]) },
                read_eq_seq_lookup,
            ],
            body: Some(Expr::ite(
                vir!{ [lo_expr] >= [hi_expr] },
                Expr::Seq(seq_type.clone(), vec![], vir::Position::default()),
                Expr::ContainerOp(
                    ContainerOpKind::SeqConcat,
                    box Expr::Seq(
                        seq_type.clone(),
                        vec![
                            read_lo,
                        ],
                        vir::Position::default(),
                    ),
                    box Expr::func_app(
                        slice_helper_name,
                        vec![
                            self_expr,
                            vir!{ [lo_expr] + [Expr::from(1)] },
                            hi_expr,
                        ],
                        vec![
                            vir_local!{ slice: {self_snap_ty} },
                            lo,
                            hi,
                        ],
                        seq_type,
                        vir::Position::default(),
                    ),
                    vir::Position::default(),
                )
            )),
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
