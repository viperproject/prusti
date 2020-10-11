// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ::log::{debug, trace};
use crate::encoder::borrows::{compute_procedure_contract, ProcedureContract, ProcedureContractMirDef};
use crate::encoder::builtin_encoder::BuiltinEncoder;
use crate::encoder::builtin_encoder::BuiltinFunctionKind;
use crate::encoder::builtin_encoder::BuiltinMethodKind;
use crate::encoder::errors::{ErrorCtxt, ErrorManager, EncodingError};
use crate::encoder::foldunfold;
use crate::encoder::places;
use crate::encoder::expires_first_domain;
use crate::encoder::procedure_encoder::ProcedureEncoder;
use crate::encoder::pure_function_encoder::PureFunctionEncoder;
use crate::encoder::stub_function_encoder::StubFunctionEncoder;
use crate::encoder::spec_encoder::encode_spec_assertion;
use crate::encoder::snapshot_encoder::{Snapshot, SnapshotEncoder};
use crate::encoder::type_encoder::{
    compute_discriminant_values, compute_discriminant_bounds, TypeEncoder};
use prusti_common::vir;
use prusti_common::vir::WithIdentifier;
use prusti_common::config;
use prusti_common::report::log;
// use prusti_interface::constants::PRUSTI_SPEC_ATTR;
use prusti_interface::data::ProcedureDefId;
use prusti_interface::environment::Environment;
use prusti_interface::specs::typed;
use prusti_interface::specs::typed::SpecificationId;
use prusti_interface::utils::{has_spec_only_attr, read_prusti_attrs, has_prusti_attr};
use prusti_interface::PrustiError;
// use prusti_interface::specs::{
//     SpecID, SpecificationSet, TypedAssertion,
//     TypedSpecificationMap, TypedSpecificationSet,
// };
use rustc_hir as hir;
use rustc_hir::def_id::DefId;
// use rustc::middle::const_val::ConstVal;
use rustc_middle::mir;
// use rustc::mir::interpret::GlobalId;
use rustc_middle::ty;
use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::io::Write;
use std::mem;
// use syntax::ast;
use rustc_ast::ast;
// use viper;
use crate::encoder::stub_procedure_encoder::StubProcedureEncoder;
use std::ops::AddAssign;
use ::log::info;
use std::convert::TryInto;
use std::borrow::Borrow;

/// A reference to a procedure specification.
///
/// TODO: Move this type and the associated functions into a separate file.
#[derive(Debug)]
enum SpecIdRef {
    Precondition(SpecificationId),
    Postcondition(SpecificationId),
    Pledge { lhs: Option<SpecificationId>, rhs: SpecificationId },
}

const SNAPSHOT_MIRROR_DOMAIN: &str = "$SnapshotMirrors$";

pub struct Encoder<'v, 'tcx: 'v> {
    env: &'v Environment<'tcx>,
    spec: &'v typed::SpecificationMap<'tcx>,
    extern_spec: &'v typed::ExternSpecificationMap<'tcx>,
    error_manager: RefCell<ErrorManager<'tcx>>,
    procedure_contracts: RefCell<HashMap<ProcedureDefId, ProcedureContractMirDef<'tcx>>>,
    builtin_methods: RefCell<HashMap<BuiltinMethodKind, vir::BodylessMethod>>,
    builtin_functions: RefCell<HashMap<BuiltinFunctionKind, vir::Function>>,
    procedures: RefCell<HashMap<ProcedureDefId, vir::CfgMethod>>,
    pure_function_bodies: RefCell<HashMap<(ProcedureDefId, String), vir::Expr>>,
    pure_functions: RefCell<HashMap<(ProcedureDefId, String), vir::Function>>,
    /// Stub pure functions. Generated when an impure Rust function is invoked
    /// where a pure function is required.
    stub_pure_functions: RefCell<HashMap<(ProcedureDefId, String), vir::Function>>,
    type_predicate_names: RefCell<HashMap<ty::TyKind<'tcx>, String>>,
    type_invariant_names: RefCell<HashMap<ty::TyKind<'tcx>, String>>,
    type_tag_names: RefCell<HashMap<ty::TyKind<'tcx>, String>>,
    predicate_types: RefCell<HashMap<String, ty::Ty<'tcx>>>,
    type_predicates: RefCell<HashMap<String, vir::Predicate>>,
    type_invariants: RefCell<HashMap<String, vir::Function>>,
    type_tags: RefCell<HashMap<String, vir::Function>>,
    type_discriminant_funcs: RefCell<HashMap<String, vir::Function>>,
    memory_eq_funcs: RefCell<HashMap<String, Option<vir::Function>>>,
    fields: RefCell<HashMap<String, vir::Field>>,
    snapshots: RefCell<HashMap<String, Box<Snapshot>>>, // maps predicate names to snapshots
    type_snapshots: RefCell<HashMap<String, String>>, // maps snapshot names to predicate names
    snap_mirror_funcs: RefCell<HashMap<String, Option<vir::DomainFunc>>>,
    /// For each instantiation of each closure: DefId, basic block index, statement index, operands
    closure_instantiations: HashMap<
        DefId,
        Vec<(
            ProcedureDefId,
            mir::Location,
            Vec<mir::Operand<'tcx>>,
            Vec<ty::Ty<'tcx>>,
        )>,
    >,
    encoding_queue: RefCell<Vec<(ProcedureDefId, Vec<(ty::Ty<'tcx>, ty::Ty<'tcx>)>)>>,
    vir_program_before_foldunfold_writer: RefCell<Box<Write>>,
    vir_program_before_viper_writer: RefCell<Box<Write>>,
    pub typaram_repl: RefCell<Vec<HashMap<ty::Ty<'tcx>, ty::Ty<'tcx>>>>,
    encoding_errors_counter: RefCell<usize>,
}

impl<'v, 'tcx> Encoder<'v, 'tcx> {
    pub fn new(env: &'v Environment<'tcx>, spec: &'v typed::SpecificationMap<'tcx>,
               extern_spec: &'v typed::ExternSpecificationMap<'tcx>) -> Self {
        let source_path = env.source_path();
        let source_filename = source_path.file_name().unwrap().to_str().unwrap();
        let vir_program_before_foldunfold_writer = RefCell::new(
            log::build_writer(
                "vir_program_before_foldunfold",
                format!("{}.vir", source_filename),
            )
            .ok()
            .unwrap(),
        );
        let vir_program_before_viper_writer = RefCell::new(
            log::build_writer(
                "vir_program_before_viper",
                format!("{}.vir", source_filename),
            )
            .ok()
            .unwrap(),
        );

        Encoder {
            env,
            spec,
            extern_spec,
            error_manager: RefCell::new(ErrorManager::new(env.codemap())),
            procedure_contracts: RefCell::new(HashMap::new()),
            builtin_methods: RefCell::new(HashMap::new()),
            builtin_functions: RefCell::new(HashMap::new()),
            procedures: RefCell::new(HashMap::new()),
            pure_function_bodies: RefCell::new(HashMap::new()),
            pure_functions: RefCell::new(HashMap::new()),
            stub_pure_functions: RefCell::new(HashMap::new()),
            type_predicate_names: RefCell::new(HashMap::new()),
            type_invariant_names: RefCell::new(HashMap::new()),
            type_tag_names: RefCell::new(HashMap::new()),
            predicate_types: RefCell::new(HashMap::new()),
            type_predicates: RefCell::new(HashMap::new()),
            type_invariants: RefCell::new(HashMap::new()),
            type_tags: RefCell::new(HashMap::new()),
            type_discriminant_funcs: RefCell::new(HashMap::new()),
            memory_eq_funcs: RefCell::new(HashMap::new()),
            fields: RefCell::new(HashMap::new()),
            closure_instantiations: HashMap::new(),
            encoding_queue: RefCell::new(vec![]),
            vir_program_before_foldunfold_writer,
            vir_program_before_viper_writer,
            typaram_repl: RefCell::new(Vec::new()),
            snapshots: RefCell::new(HashMap::new()),
            type_snapshots: RefCell::new(HashMap::new()),
            snap_mirror_funcs: RefCell::new(HashMap::new()),
            encoding_errors_counter: RefCell::new(0),
        }
    }

    pub fn log_vir_program_before_foldunfold<S: ToString>(&self, program: S) {
        let mut writer = self.vir_program_before_foldunfold_writer.borrow_mut();
        writer
            .write_all(program.to_string().as_bytes())
            .ok()
            .unwrap();
        writer
            .write_all("\n\n".to_string().as_bytes())
            .ok()
            .unwrap();
        writer.flush().ok().unwrap();
    }

    pub fn log_vir_program_before_viper<S: ToString>(&self, program: S) {
        let mut writer = self.vir_program_before_viper_writer.borrow_mut();
        writer
            .write_all(program.to_string().as_bytes())
            .ok()
            .unwrap();
        writer
            .write_all("\n\n".to_string().as_bytes())
            .ok()
            .unwrap();
        writer.flush().ok().unwrap();
    }

    fn initialize(&mut self) {
        self.collect_closure_instantiations();
        // These are used in optimization passes
        self.encode_builtin_method_def(BuiltinMethodKind::HavocBool);
        self.encode_builtin_method_def(BuiltinMethodKind::HavocInt);
        self.encode_builtin_method_def(BuiltinMethodKind::HavocRef);
    }

    pub fn env(&self) -> &'v Environment<'tcx> {
        self.env
    }

    pub fn spec(&self) -> &'v typed::SpecificationMap<'tcx> {
        self.spec
    }

    /// Returns the def_id of the element containing the specifications.
    /// This can be different from the def_id that was passed in if the
    /// specifications were externally declared.
    pub fn get_specification_def_id(&self, def_id: &'v ProcedureDefId) -> &'v ProcedureDefId {
        if def_id.is_local() && self.extern_spec.contains_key(def_id) &&
            self.get_procedure_specs(*def_id).is_some() {
            self.register_encoding_error(EncodingError::Incorrect(
                format!("external specification found for already specified function"),
                rustc_span::MultiSpan::from_span(self.env.tcx().def_span(self.extern_spec.get(def_id).unwrap().1))));
        }
        if (self.extern_spec.contains_key(def_id)) {
            &self.extern_spec.get(def_id).unwrap().1
        } else {
            def_id
        }
    }

    pub fn error_manager(&self) -> RefMut<ErrorManager<'tcx>> {
        self.error_manager.borrow_mut()
    }

    pub fn get_viper_program(&self) -> vir::Program {
        vir::Program {
            domains: self.get_used_viper_domains(),
            fields: self.get_used_viper_fields(),
            builtin_methods: self.get_used_builtin_methods(),
            methods: self.get_used_viper_methods(),
            functions: self.get_used_viper_functions(),
            viper_predicates: self.get_used_viper_predicates(),
        }
    }

    pub(in crate::encoder) fn register_encoding_error(&self, encoding_error: EncodingError) {
        debug!("Encoding error: {:?}", encoding_error);
        let prusti_error: PrustiError = encoding_error.into();
        if prusti_error.is_error() {
            self.encoding_errors_counter.borrow_mut().add_assign(1);
        }
        prusti_error.emit(self.env);
    }

    pub fn count_encoding_errors(&self) -> usize {
        *self.encoding_errors_counter.borrow()
    }

    pub fn get_used_viper_domains(&self) -> Vec<vir::Domain> {
        let mirrors: Vec<_> = self
            .snap_mirror_funcs
            .borrow()
            .values()
            .filter_map(|f| f.clone())
            .collect();

        let mut domains: Vec<vir::Domain> = self
            .snapshots
            .borrow()
            .values()
            .into_iter()
            .filter_map(|s| s.get_domain())
            .collect();
        if !mirrors.is_empty() {
            domains.push(vir::Domain {
                name: SNAPSHOT_MIRROR_DOMAIN.to_string(),
                functions: mirrors,
                axioms: vec![],
                type_vars: vec![],
            });
        }
        domains.push(expires_first_domain::DOMAIN.clone());
        domains.sort_by_key(|d| d.get_identifier());
        domains
    }

    fn get_used_viper_fields(&self) -> Vec<vir::Field> {
        let mut fields: Vec<_> = self.fields.borrow().values().cloned().collect();
        fields.sort_by_key(|f| f.get_identifier());
        fields
    }

    fn get_used_viper_functions(&self) -> Vec<vir::Function> {
        let mut functions: Vec<_> = vec![];
        for function in self.builtin_functions.borrow().values() {
            functions.push(function.clone());
        }
        for function in self.pure_functions.borrow().values() {
            functions.push(function.clone());
        }
        for function in self.stub_pure_functions.borrow().values() {
            functions.push(function.clone());
        }
        for function in self.type_invariants.borrow().values() {
            functions.push(function.clone());
        }
        for function in self.type_tags.borrow().values() {
            functions.push(function.clone());
        }
        for function in self.type_discriminant_funcs.borrow().values() {
            functions.push(function.clone());
        }
        for function in self.memory_eq_funcs.borrow().values() {
            functions.push(function.as_ref().unwrap().clone());
        }
        for snap in self.snapshots.borrow().values() {
            for function in snap.get_functions() {
                functions.push(function);
            }
        }
        functions.sort_by_key(|f| f.get_identifier());
        functions
    }

    fn get_used_viper_predicates(&self) -> Vec<vir::Predicate> {
        let mut predicates: Vec<_> = self.type_predicates.borrow().values().cloned().collect();

        // Add a predicate that represents the dead loan token.
        predicates.push(vir::Predicate::Bodyless(
            "DeadBorrowToken$".to_string(),
            vir::LocalVar {
                name: "borrow".to_string(),
                typ: vir::Type::Int,
            },
        ));

        predicates.sort_by_key(|f| f.get_identifier());
        predicates
    }

    pub fn get_used_viper_predicates_map(&self) -> HashMap<String, vir::Predicate> {
        self.type_predicates.borrow().clone()
    }

    fn get_used_builtin_methods(&self) -> Vec<vir::BodylessMethod> {
        self.builtin_methods.borrow().values().cloned().collect()
    }

    fn get_used_viper_methods(&self) -> Vec<vir::CfgMethod> {
        self.procedures.borrow().values().cloned().collect()
    }

    fn collect_closure_instantiations(&mut self) {
        debug!("Collecting closure instantiations...");
        let tcx = self.env().tcx();
        let mut closure_instantiations: HashMap<DefId, Vec<_>> = HashMap::new();
        let crate_num = hir::def_id::LOCAL_CRATE;
        for &mir_def_id in tcx.mir_keys(crate_num).iter() {
            // Do not inspect `mir_def_id`s for which we cannot build a MIR.
            // The match is a simplification of:
            // https://github.com/rust-lang/rust/blob/4e3eb5249340898e4380ffe37e7ed8c6b2afdbf9/compiler/rustc_mir_build/src/build/mod.rs#L36
            let id = tcx.hir().local_def_id_to_hir_id(mir_def_id);
            let inspect = match tcx.hir().get(id) {
                hir::Node::Expr(hir::Expr { kind: hir::ExprKind::Closure(..), .. })
                | hir::Node::Item(hir::Item { kind: hir::ItemKind::Fn(..), .. })
                | hir::Node::Item(hir::Item { kind: hir::ItemKind::Static(..), .. })
                | hir::Node::Item(hir::Item { kind: hir::ItemKind::Const(..), .. })
                | hir::Node::ImplItem(hir::ImplItem { kind: hir::ImplItemKind::Fn(..), .. })
                | hir::Node::ImplItem(hir::ImplItem { kind: hir::ImplItemKind::Const(..), .. })
                | hir::Node::TraitItem(hir::TraitItem { kind: hir::TraitItemKind::Fn(..), .. })
                | hir::Node::TraitItem(hir::TraitItem {
                                      kind: hir::TraitItemKind::Const(_, Some(..)),
                                      ..
                                  }) => true,
                _ => false
            };
            if !inspect {
                debug!("Skip collecting closure instantiations in {:?}", mir_def_id);
                continue;
            }

            trace!("Collecting closure instantiations in mir {:?}", mir_def_id);
            let ref_mir = self.env().mir(mir_def_id);
            let mir = &*ref_mir;
            for (bb_index, bb_data) in mir.basic_blocks().iter_enumerated() {
                for (stmt_index, stmt) in bb_data.statements.iter().enumerate() {
                    if let mir::StatementKind::Assign(
                        box (
                            _,
                            mir::Rvalue::Aggregate(
                                box mir::AggregateKind::Closure(cl_def_id, _),
                                ref operands,
                            ),
                        )
                    ) = stmt.kind
                    {
                        if !self.env().has_prusti_attribute(cl_def_id, "spec_only") {
                            continue;
                        }
                        trace!("Found closure instantiation: {:?}", stmt);
                        let operand_tys = operands.iter().map(
                            |operand| operand.ty(mir, tcx)
                        ).collect();
                        let instantiations =
                            closure_instantiations.entry(cl_def_id).or_insert(vec![]);
                        instantiations.push((
                            mir_def_id.to_def_id(),
                            mir::Location {
                                block: bb_index,
                                statement_index: stmt_index,
                            },
                            operands.clone(),
                            operand_tys
                        ))
                    }
                }
            }
        }
        debug!("closure_instantiations: {:?}", closure_instantiations);
        self.closure_instantiations = closure_instantiations;
    }

    pub fn get_closure_instantiations(
        &self,
        closure_def_id: DefId,
    ) -> Vec<(
        ProcedureDefId,
        mir::Location,
        Vec<mir::Operand<'tcx>>,
        Vec<ty::Ty<'tcx>>,
    )> {
        trace!("Get closure instantiations for {:?}", closure_def_id);
        match self.closure_instantiations.get(&closure_def_id) {
            Some(result) => result.clone(),
            None => vec![],
        }
    }

    pub fn get_single_closure_instantiation(
        &self,
        closure_def_id: DefId,
    ) -> Option<(
        ProcedureDefId,
        mir::Location,
        Vec<mir::Operand<'tcx>>,
        Vec<ty::Ty<'tcx>>,
    )> {
        let mut instantiations = self.get_closure_instantiations(closure_def_id);
        assert!(instantiations.len() <= 1);
        if instantiations.is_empty() {
            None
        } else {
            Some(instantiations.remove(0))
        }
    }

    /// Is the closure specified with the `def_id` is spec only?
    pub fn is_spec_closure(&self, def_id: DefId) -> bool {
        use rustc_ast::ast;
        has_spec_only_attr(self.env().tcx().get_attrs(def_id))
    }

    /// Return the specification ids that are attached to `def_id` with one of the following
    /// attributes:
    /// * `prusti::pre_spec_id_ref="..."` for preconditions,
    /// * `prusti::post_spec_id_ref="..."` for postconditions,
    /// * `prusti::pledge_spec_id_ref="..."` for pledges.
    fn get_procedure_spec_ids(&self, def_id: DefId) -> Vec<SpecIdRef> {
        let mut spec_id_refs = vec![];
        let attrs = self.env().tcx().get_attrs(def_id);

        let parse_spec_id = |spec_id: String| -> SpecificationId {
            spec_id.try_into().expect(
                &format!("cannot parse the spec_id attached to {:?}", def_id)
            )
        };

        spec_id_refs.extend(
            read_prusti_attrs("pre_spec_id_ref", attrs).into_iter().map(
                |raw_spec_id| SpecIdRef::Precondition(parse_spec_id(raw_spec_id))
            )
        );
        spec_id_refs.extend(
            read_prusti_attrs("post_spec_id_ref", attrs).into_iter().map(
                |raw_spec_id| SpecIdRef::Postcondition(parse_spec_id(raw_spec_id))
            )
        );
        spec_id_refs.extend(
            read_prusti_attrs("pledge_spec_id_ref", attrs).into_iter().map(
                |value| {
                    let mut value = value.splitn(2, ":");
                    let raw_lhs_spec_id = value.next().unwrap();
                    let raw_rhs_spec_id = value.next().unwrap();
                    let lhs_spec_id = if !raw_lhs_spec_id.is_empty() {
                        Some(parse_spec_id(raw_lhs_spec_id.to_string()))
                    } else {
                        None
                    };
                    let rhs_spec_id = parse_spec_id(raw_rhs_spec_id.to_string());
                    SpecIdRef::Pledge{ lhs: lhs_spec_id, rhs: rhs_spec_id }
                }
            )
        );
        debug!("Function {:?} has specification ids {:?}", def_id, spec_id_refs);
        spec_id_refs
    }

    /// Get the loop invariant attached to a function with a
    /// `prusti::loop_body_invariant_spec` attribute.
    pub fn get_loop_specs(&self, def_id: DefId) -> Vec<SpecificationId> {
        let attrs = self.env().tcx().get_attrs(def_id);
        debug_assert!(has_prusti_attr(attrs, "loop_body_invariant_spec"));
        read_prusti_attrs("spec_id", attrs).into_iter().map(
            |raw_spec_id| raw_spec_id.try_into().expect(
                &format!("cannot parse the spec_id attached to {:?}", def_id)
            )
        ).collect()
    }

    /// Get the specifications attached to the `def_id` function.
    pub fn get_procedure_specs(&self, def_id: DefId) -> Option<typed::SpecificationSet<'tcx>> {
        debug!("get spec for: {:?}", def_id);
        // Currently, we don't support specifications for external functions.
        // Since we have a collision of PRUSTI_SPEC_ATTR between different crates, we manually check
        // that the def_id does not point to an external crate.
        if !def_id.is_local() {
            return None;
        }
        let refs = self.get_procedure_spec_ids(def_id);
        if refs.is_empty() {
            None
        } else {
            let mut pres = Vec::new();
            let mut posts = Vec::new();
            let mut pledges = Vec::new();
            for spec_id_ref in refs {
                match spec_id_ref {
                    SpecIdRef::Precondition(spec_id) => {
                        pres.push(self.spec().get(&spec_id).unwrap().clone());
                    }
                    SpecIdRef::Postcondition(spec_id) => {
                        posts.push(self.spec().get(&spec_id).unwrap().clone());
                    }
                    SpecIdRef::Pledge{ lhs, rhs } => {
                        pledges.push(typed::Pledge {
                            rhs: self.spec().get(&rhs).unwrap().clone(),
                        })
                    }
                }
            }
            Some(typed::SpecificationSet::Procedure(typed::ProcedureSpecification::new(pres, posts, pledges)))
        }
    }

    fn get_procedure_contract(&self, proc_def_id: ProcedureDefId) -> ProcedureContractMirDef<'tcx> {
        let opt_fun_spec = self.get_procedure_specs(proc_def_id);
        let fun_spec = match opt_fun_spec {
            Some(fun_spec) => fun_spec.clone(),
            None => {
                debug!("Procedure {:?} has no specification", proc_def_id);
                typed::SpecificationSet::Procedure(typed::ProcedureSpecification::empty())
            }
        };
        compute_procedure_contract(proc_def_id, self.env().tcx(), fun_spec, None)
    }

    pub fn get_procedure_contract_for_def(
        &self,
        proc_def_id: ProcedureDefId,
    ) -> ProcedureContract<'tcx> {
        self.procedure_contracts
            .borrow_mut()
            .entry(proc_def_id)
            .or_insert_with(|| self.get_procedure_contract(proc_def_id))
            .to_def_site_contract()
    }

    pub fn get_procedure_contract_for_call(
        &self,
        self_ty: Option<&'tcx ty::TyS<'tcx>>,
        proc_def_id: ProcedureDefId,
        args: &Vec<places::Local>,
        target: places::Local,
    ) -> ProcedureContract<'tcx> {
        // get specification on trait declaration method or inherent impl
        let fun_spec = if let Some(spec) = self.get_procedure_specs(proc_def_id) {
            spec.clone()
        } else {
            debug!("Procedure {:?} has no specification", proc_def_id);
            typed::SpecificationSet::Procedure(typed::ProcedureSpecification::empty())
        };

        let tymap = self.typaram_repl.borrow_mut();

        assert_eq!(tymap.len(), 1, "tymap.len() = {}, but should be 1", tymap.len());

        // get receiver object base type
        let mut impl_spec = typed::SpecificationSet::Procedure(typed::ProcedureSpecification::empty());

        // let mut self_ty = None;

        // for (key, val) in tymap[0].iter() {
        //     if key.is_self() {   // FIXME: This check does not work anymore.
        //         self_ty = Some(val.clone());
        //     }
        // }

        if let Some(ty) = self_ty {
            if let Some(id) = self.env().tcx().trait_of_item(proc_def_id) {
                let proc_name = self.env().tcx().item_name(proc_def_id);
                let procs = self.env().get_trait_method_decl_for_type(ty, id, proc_name);
                if procs.len() == 1 {
                    // FIXME(@jakob): if several methods are found, we currently don't know which
                    // one to pick.
                    let item = procs[0];
                    if let Some(spec) = self.get_procedure_specs(item.def_id) {
                        impl_spec = spec.clone();
                    } else {
                        debug!("Procedure {:?} has no specification", item.def_id);
                    }
                }
            }
        }

        // merge specifications
        let final_spec = fun_spec.refine(&impl_spec);

        let contract =
            compute_procedure_contract(proc_def_id, self.env().tcx(), final_spec, Some(&tymap[0]));
        contract.to_call_site_contract(args, target)
    }

    /// Encodes a value in a field if the base expression is a reference or
    /// a primitive types.
    /// For composed data structures, the base expression is returned.
    pub fn encode_value_expr(&self, base: vir::Expr, ty: ty::Ty<'tcx>) -> vir::Expr {
        match ty.kind() {
            ty::TyKind::Adt(_, _)
            | ty::TyKind::Tuple(_) => {
                base // don't use a field for tuples and ADTs
            }
            _ => {
                let value_field = self.encode_value_field(ty);
                base.field(value_field)
            }
        }
    }

    pub fn encode_value_field(&self, ty: ty::Ty<'tcx>) -> vir::Field {
        let type_encoder = TypeEncoder::new(self, ty);
        let field = type_encoder.encode_value_field();
        self.fields
            .borrow_mut()
            .entry(field.name.clone())
            .or_insert_with(|| field.clone());
        field
    }

    pub fn encode_raw_ref_field(&self, viper_field_name: String, ty: ty::Ty<'tcx>) -> vir::Field {
        let type_name = self.encode_type_predicate_use(ty).unwrap(); // will panic if attempting to encode unsupported type
        self.fields
            .borrow_mut()
            .entry(viper_field_name.clone())
            .or_insert_with(|| {
                // Do not store the name of the type in self.fields
                vir::Field::new(
                    viper_field_name.clone(),
                    vir::Type::TypedRef("".to_string()),
                )
            });
        vir::Field::new(viper_field_name, vir::Type::TypedRef(type_name))
    }

    pub fn encode_dereference_field(&self, ty: ty::Ty<'tcx>) -> vir::Field {
        self.encode_raw_ref_field("val_ref".to_string(), ty)
    }

    pub fn encode_struct_field(&self, field_name: &str, ty: ty::Ty<'tcx>) -> vir::Field {
        let viper_field_name = format!("f${}", field_name);
        self.encode_raw_ref_field(viper_field_name, ty)
    }

    /// Creates a field that corresponds to the enum variant ``index``.
    pub fn encode_enum_variant_field(&self, index: &str) {
        let name = format!("enum_{}", index);
        let mut fields = self.fields.borrow_mut();
        if !fields.contains_key(&name) {
            let field = vir::Field::new(name.clone(), vir::Type::TypedRef("".to_string()));
            fields.insert(name, field);
        }
    }

    pub fn encode_discriminant_field(&self) -> vir::Field {
        let name = "discriminant";
        let field = vir::Field::new(name, vir::Type::Int);
        self.fields
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| field.clone());
        field
    }

    pub fn encode_discriminant_func_app(
        &self,
        place: vir::Expr,
        adt_def: &ty::AdtDef,
    ) -> vir::Expr {
        let typ = place.get_type().clone();
        let mut name = typ.name();
        name.push_str("$$discriminant$$");
        let self_local_var = vir::LocalVar::new("self", typ);
        self.type_discriminant_funcs
            .borrow_mut()
            .entry(name.clone())
            .or_insert_with(|| {
                let predicate_name = place.get_type().name();
                let precondition = vir::Expr::predicate_access_predicate(
                    predicate_name,
                    self_local_var.clone().into(),
                    vir::PermAmount::Read,
                );
                let result = vir::LocalVar::new("__result", vir::Type::Int);
                let postcondition = compute_discriminant_bounds(
                    adt_def, self.env.tcx(), &result.into());
                let discr_field = self.encode_discriminant_field();
                let self_local_var_expr: vir::Expr = self_local_var.clone().into();
                let function = vir::Function {
                    name: name.clone(),
                    formal_args: vec![self_local_var.clone()],
                    return_type: vir::Type::Int,
                    pres: vec![precondition],
                    posts: vec![postcondition],
                    body: Some(self_local_var_expr.field(discr_field)),
                };
                let final_function = foldunfold::add_folding_unfolding_to_function(
                    function,
                    self.get_used_viper_predicates_map(),
                );
                final_function.unwrap()
            });
        vir::Expr::FuncApp(
            name,
            vec![place],
            vec![self_local_var],
            vir::Type::Int,
            vir::Position::default(),
        )
    }

    fn encode_memory_eq_tuple(
        &self,
        first: vir::Expr,
        second: vir::Expr,
        elems: ty::subst::SubstsRef<'tcx>,
    ) -> vir::Expr {
        let mut conjuncts = Vec::new();
        for (field_num, arg) in elems.iter().enumerate() {
            let ty = arg.expect_ty();
            let field_name = format!("tuple_{}", field_num);
            let field = self.encode_raw_ref_field(field_name, ty);
            let first_field = first.clone().field(field.clone());
            let second_field = second.clone().field(field);
            let eq = self.encode_memory_eq_func_app(
                second_field, first_field, ty, vir::Position::default());
            conjuncts.push(eq);
        }
        vir::ExprIterator::conjoin(&mut conjuncts.into_iter())
    }

    fn encode_memory_eq_adt(
        &self,
        first: vir::Expr,
        second: vir::Expr,
        adt_def: &ty::AdtDef,
        subst: ty::subst::SubstsRef<'tcx>,
    ) -> vir::Expr {
        let tcx = self.env().tcx();
        let num_variants = adt_def.variants.len();
        let mut conjuncts = Vec::new();
        if num_variants == 1 {
            // A struct.
            // TODO this should eventually be replaced by using snapshots?
            // conjuncts.push(self.encode_adt_snap_eq_call(first, second));
            let variant_def = &adt_def.non_enum_variant();
            for field in &variant_def.fields {
                let field_name = &field.ident.as_str();
                let field_ty = field.ty(tcx, subst);
                let elem_field = self.encode_struct_field(field_name, field_ty);
                let first_field = first.clone().field(elem_field.clone());
                let second_field = second.clone().field(elem_field);
                let eq = self.encode_memory_eq_func_app(
                    first_field, second_field, field_ty, vir::Position::default());
                conjuncts.push(eq);
            }
        } else {
            // An enum.
            let discr_field = self.encode_discriminant_field();
            let first_discriminant = first.clone().field(discr_field.clone());
            let second_discriminant = second.clone().field(discr_field);
            conjuncts.push(vir::Expr::eq_cmp(first_discriminant.clone(), second_discriminant));
            let discriminant_values = compute_discriminant_values(adt_def, tcx);
            let variants = adt_def.variants
                .iter()
                .zip(discriminant_values)
                .map(|(variant_def, variant_index)| {
                    let guard = vir::Expr::eq_cmp(
                        first_discriminant.clone(),
                        variant_index.into(),
                    );
                    let variant_name = &variant_def.ident.as_str();
                    let first_location = first.clone().variant(variant_name);
                    let second_location = second.clone().variant(variant_name);
                    let eq = self.encode_memory_eq_func_app_variant(
                        first_location, second_location, variant_def, subst,
                        vir::Position::default());
                    vir::Expr::implies(guard, eq)
                });
            conjuncts.extend(variants);
        }
        vir::ExprIterator::conjoin(&mut conjuncts.into_iter())
    }

    fn encode_memory_eq_func_body(
        &self,
        first: vir::Expr,
        second: vir::Expr,
        self_ty: ty::Ty<'tcx>
    ) -> Option<vir::Expr> {
        let eq = match self_ty.kind() {
            ty::TyKind::Bool
            | ty::TyKind::Int(_)
            | ty::TyKind::Uint(_)
            | ty::TyKind::Char => {
                let field = self.encode_value_field(self_ty);
                let first_field = first.clone().field(field.clone());
                let second_field = second.clone().field(field);
                Some(vir::Expr::eq_cmp(first_field, second_field))
            }
            ty::TyKind::Adt(adt_def, subst) if !adt_def.is_box() => {
                // TODO: If adt_def contains fields of unsupported type,
                // we should return None.
                Some(self.encode_memory_eq_adt(first.clone(), second.clone(), adt_def, subst))
            }
            ty::TyKind::Tuple(elems) => {
                Some(self.encode_memory_eq_tuple(first.clone(), second.clone(), elems))
            }
            ty::TyKind::Param(_) => {
                None
            },

            ref x => unimplemented!("{:?}", x),
        };
        eq.map(|body| {
            vir::Expr::wrap_in_unfolding(first, vir::Expr::wrap_in_unfolding(second, body))
        })
    }

    /// Note: We generate functions already with the required unfoldings because some types are
    /// huge and fold unfold is too slow for them.
    fn encode_memory_eq_func(&self, name: String, self_ty: ty::Ty<'tcx>) {
        assert!(!self.memory_eq_funcs.borrow().contains_key(&name));
        // Mark that we started encoding this function to avoid infinite recursion.
        self.memory_eq_funcs.borrow_mut().insert(name.clone(), None);

        let type_name = self.encode_type_predicate_use(self_ty).unwrap(); // will panic if attempting to encode unsupported type
        let typ = vir::Type::TypedRef(type_name.clone());
        let first_local_var = vir::LocalVar::new("self", typ.clone());
        let second_local_var = vir::LocalVar::new("other", typ);
        let precondition = vec![
            vir::Expr::predicate_access_predicate(
                type_name.clone(),
                first_local_var.clone().into(),
                vir::PermAmount::Read,
            ),
            vir::Expr::predicate_access_predicate(
                type_name,
                second_local_var.clone().into(),
                vir::PermAmount::Read,
            ),
        ];
        let body = self.encode_memory_eq_func_body(
            first_local_var.clone().into(), second_local_var.clone().into(), self_ty);
        let function = vir::Function {
            name: name.clone(),
            formal_args: vec![first_local_var, second_local_var],
            return_type: vir::Type::Bool,
            pres: precondition,
            posts: vec![],
            body: body,
        };
        self.memory_eq_funcs.borrow_mut().insert(name, Some(function));
    }

    /// Note: We generate functions already with the required unfoldings because some types are
    /// huge and fold unfold is too slow for them.
    fn encode_memory_eq_func_variant(
        &self,
        name: String,
        typ: vir::Type,
        self_variant: &ty::VariantDef,
        subst: ty::subst::SubstsRef<'tcx>,
    ) {
        assert!(!self.memory_eq_funcs.borrow().contains_key(&name));
        // Mark that we started encoding this function to avoid infinite recursion.
        self.memory_eq_funcs.borrow_mut().insert(name.clone(), None);
        let tcx = self.env().tcx();
        let type_name = typ.name();
        let first_local_var = vir::LocalVar::new("self", typ.clone());
        let second_local_var = vir::LocalVar::new("other", typ);
        let precondition = vec![
            vir::Expr::predicate_access_predicate(
                type_name.clone(),
                first_local_var.clone().into(),
                vir::PermAmount::Read,
            ),
            vir::Expr::predicate_access_predicate(
                type_name,
                second_local_var.clone().into(),
                vir::PermAmount::Read,
            ),
        ];
        let mut conjuncts = self_variant.fields
            .iter()
            .map(|field| {
                let field_name = &field.ident.as_str();
                let field_ty = field.ty(tcx, subst);
                let encoded_field = self.encode_struct_field(field_name, field_ty);
                let first_field = vir::Expr::from(first_local_var.clone())
                    .field(encoded_field.clone());
                let second_field = vir::Expr::from(second_local_var.clone())
                    .field(encoded_field.clone());
                self.encode_memory_eq_func_app(
                    first_field, second_field, field_ty, vir::Position::default())
            });
        let conjunction = vir::ExprIterator::conjoin(&mut conjuncts);
        let unfolded_second = vir::Expr::wrap_in_unfolding(
            second_local_var.clone().into(), conjunction);
        let unfolded_first = vir::Expr::wrap_in_unfolding(
            first_local_var.clone().into(), unfolded_second);
        let body = Some(unfolded_first);
        let function = vir::Function {
            name: name.clone(),
            formal_args: vec![first_local_var, second_local_var],
            return_type: vir::Type::Bool,
            pres: precondition,
            posts: vec![],
            body: body,
        };
        self.memory_eq_funcs.borrow_mut().insert(name, Some(function));
    }

    pub fn encode_memory_eq_func_app(
        &self,
        first: vir::Expr,
        second: vir::Expr,
        self_ty: ty::Ty<'tcx>,
        position: vir::Position,
    ) -> vir::Expr {
        let typ = first.get_type().clone();
        assert!(&typ == second.get_type());
        let mut name = typ.name();
        name.push_str("$$memory_eq$$");
        if !self.memory_eq_funcs.borrow().contains_key(&name) {
            self.encode_memory_eq_func(name.clone(), self_ty);
        }
        let first_local_var = vir::LocalVar::new("self", typ.clone());
        let second_local_var = vir::LocalVar::new("other", typ);
        vir::Expr::FuncApp(
            name,
            vec![first, second],
            vec![first_local_var, second_local_var],
            vir::Type::Bool,
            position,
        )
    }

    pub fn encode_memory_eq_func_app_variant(
        &self,
        first: vir::Expr,
        second: vir::Expr,
        self_variant: &ty::VariantDef,
        subst: ty::subst::SubstsRef<'tcx>,
        position: vir::Position,
    ) -> vir::Expr {
        let typ = first.get_type().clone();
        assert!(&typ == second.get_type());
        let mut name = typ.name();
        name.push_str("$$memory_eq$$");
        if !self.memory_eq_funcs.borrow().contains_key(&name) {
            self.encode_memory_eq_func_variant(name.clone(), typ.clone(), self_variant, subst);
        }
        let first_local_var = vir::LocalVar::new("self", typ.clone());
        let second_local_var = vir::LocalVar::new("other", typ);
        vir::Expr::FuncApp(
            name,
            vec![first, second],
            vec![first_local_var, second_local_var],
            vir::Type::Bool,
            position,
        )
    }

    pub fn encode_builtin_method_def(&self, method_kind: BuiltinMethodKind) -> vir::BodylessMethod {
        trace!("encode_builtin_method_def({:?})", method_kind);
        if !self.builtin_methods.borrow().contains_key(&method_kind) {
            let builtin_encoder = BuiltinEncoder::new();
            let method = builtin_encoder.encode_builtin_method_def(method_kind);
            self.log_vir_program_before_viper(method.to_string());
            self.builtin_methods
                .borrow_mut()
                .insert(method_kind.clone(), method);
        }
        self.builtin_methods.borrow()[&method_kind].clone()
    }

    pub fn encode_builtin_method_use(&self, method_kind: BuiltinMethodKind) -> String {
        trace!("encode_builtin_method_use({:?})", method_kind);
        // Trigger encoding of definition
        self.encode_builtin_method_def(method_kind);
        let builtin_encoder = BuiltinEncoder::new();
        builtin_encoder.encode_builtin_method_name(method_kind)
    }

    pub fn encode_builtin_function_def(&self, function_kind: BuiltinFunctionKind) -> vir::Function {
        trace!("encode_builtin_function_def({:?})", function_kind);
        if !self.builtin_functions.borrow().contains_key(&function_kind) {
            let builtin_encoder = BuiltinEncoder::new();
            let function = builtin_encoder.encode_builtin_function_def(function_kind.clone());
            self.log_vir_program_before_viper(function.to_string());
            self.builtin_functions
                .borrow_mut()
                .insert(function_kind.clone(), function);
        }
        self.builtin_functions.borrow()[&function_kind].clone()
    }

    pub fn encode_builtin_function_use(&self, function_kind: BuiltinFunctionKind) -> String {
        trace!("encode_builtin_function_use({:?})", function_kind);
        if !self.builtin_functions.borrow().contains_key(&function_kind) {
            // Trigger encoding of definition
            self.encode_builtin_function_def(function_kind.clone());
        }
        let builtin_encoder = BuiltinEncoder::new();
        builtin_encoder.encode_builtin_function_name(&function_kind)
    }

    pub fn encode_procedure(&self, def_id: ProcedureDefId) -> Result<vir::CfgMethod, EncodingError> {
        debug!("encode_procedure({:?})", def_id);
        assert!(
            !self.env.has_prusti_attribute(def_id, "pure"),
            "procedure is marked as pure: {:?}",
            def_id
        );
        assert!(
            !self.env.has_prusti_attribute(def_id, "trusted"),
            "procedure is marked as trusted: {:?}",
            def_id
        );
        if !self.procedures.borrow().contains_key(&def_id) {
            let procedure = self.env.get_procedure(def_id);
            let proc_encoder = ProcedureEncoder::new(self, &procedure)?;
            let method = match proc_encoder.encode() {
                Ok(result) => result,
                Err(error) => {
                    self.register_encoding_error(error);
                    StubProcedureEncoder::new(self, &procedure).encode()
                },
            };
            self.log_vir_program_before_viper(method.to_string());
            self.procedures.borrow_mut().insert(def_id, method);
        }
        Ok(self.procedures.borrow()[&def_id].clone())
    }

    pub fn encode_value_or_ref_type(&self, ty: ty::Ty<'tcx>) -> vir::Type {
        let type_encoder = TypeEncoder::new(self, ty);
        type_encoder.encode_value_or_ref_type()
    }

    pub fn encode_value_type(&self, ty: ty::Ty<'tcx>) -> vir::Type {
        let type_encoder = TypeEncoder::new(self, ty);
        type_encoder.encode_value_type()
    }

    pub fn encode_type(&self, ty: ty::Ty<'tcx>) -> vir::Type {
        let type_encoder = TypeEncoder::new(self, ty);
        type_encoder.encode_type()
    }

    pub fn encode_type_bounds(&self, var: &vir::Expr, ty: ty::Ty<'tcx>) -> Vec<vir::Expr> {
        let type_encoder = TypeEncoder::new(self, ty);
        type_encoder.encode_bounds(var)
    }

    pub fn encode_assertion(
        &self,
        assertion: &typed::Assertion<'tcx>,
        mir: &mir::Body<'tcx>,
        pre_label: Option<&str>,
        target_args: &[vir::Expr],
        target_return: Option<&vir::Expr>,
        targets_are_values: bool,
        assertion_location: Option<mir::BasicBlock>,
        error: ErrorCtxt,
    ) -> vir::Expr {
        trace!("encode_assertion {:?}", assertion);
        let encoded_assertion = encode_spec_assertion(
            self,
            assertion,
            pre_label,
            target_args,
            target_return,
            targets_are_values,
            assertion_location,
        );
        encoded_assertion.set_default_pos(
            self.error_manager()
                .register(typed::Spanned::get_spans(assertion, mir, self.env().tcx()), error),
        )
    }

    pub fn encode_type_predicate_use(&self, ty: ty::Ty<'tcx>) -> Result<String, ErrorCtxt> {
        if !self.type_predicate_names.borrow().contains_key(ty.kind()) {
            let type_encoder = TypeEncoder::new(self, ty);
            let result = type_encoder.encode_predicate_use()?;
            self.type_predicate_names
                .borrow_mut()
                .insert(ty.kind().clone(), result);
            // Trigger encoding of definition
            self.encode_type_predicate_def(ty);
        }
        let predicate_name = self.type_predicate_names.borrow()[&ty.kind()].clone();
        self.predicate_types
            .borrow_mut()
            .insert(predicate_name.clone(), ty);
        Ok(predicate_name)
    }

    pub fn encode_type_predicate_def(&self, ty: ty::Ty<'tcx>) -> vir::Predicate {
        let predicate_name = self.encode_type_predicate_use(ty).unwrap();
        if !self.type_predicates.borrow().contains_key(&predicate_name) {
            let type_encoder = TypeEncoder::new(self, ty);
            let predicates = type_encoder.encode_predicate_def();
            for predicate in predicates {
                self.log_vir_program_before_viper(predicate.to_string());
                let predicate_name = predicate.name();
                self.type_predicates
                    .borrow_mut()
                    .insert(predicate_name.to_string(), predicate);
            }
        }
        self.type_predicates.borrow()[&predicate_name].clone()
    }

    pub fn encode_snapshot(&self, ty: ty::Ty<'tcx>) -> Box<Snapshot> {
        let ty = self.dereference_ty(ty);
        // will panic if attempting to encode unsupported type
        let predicate_name = self.encode_type_predicate_use(ty).unwrap();
        if !self.snapshots.borrow().contains_key(&predicate_name) {
            let encoder = SnapshotEncoder::new(
                self, ty,
                predicate_name.to_string()
            );
            let snapshot = encoder.encode();
            if snapshot.is_defined() {
                self.type_snapshots
                    .borrow_mut()
                    .insert(
                        snapshot.get_type().name().to_string(),
                        predicate_name.to_string()
                    );
            }
            self.snapshots
                .borrow_mut()
                .insert(predicate_name.to_string(), box snapshot);
        }
        self.snapshots.borrow()[&predicate_name].clone()
    }

    fn dereference_ty(&self, ty: ty::Ty<'tcx>) -> ty::Ty<'tcx> {
        match ty.kind() {
            ty::TyKind::Ref(_, ref val_ty, _) => self.dereference_ty(val_ty),
            _ => ty,
        }
    }

    /// Checks whether the given type implements structural equality
    /// by either being a primitive type or by deriving the Eq trait.
    pub fn has_structural_eq_impl(&self, ty: ty::Ty<'tcx>) -> bool {
        let ty = self.dereference_ty(ty);
        match ty.kind() {
            ty::TyKind::Bool
            | ty::TyKind::Int(_)
            | ty::TyKind::Uint(_)
            | ty::TyKind::Char
            | ty::TyKind::Tuple(_)
            | ty::TyKind::Never
            | ty::TyKind::Param(_) => true,
            ty::TyKind::Adt(_, _) => {
                self.env().tcx().has_structural_eq_impls(ty)
            }
            _ => false,
        }
    }

    pub fn encode_snapshot_use(&self, predicate_name: String) -> Box<Snapshot> {
        if !self.snapshots.borrow().contains_key(&predicate_name) {
            if !self.predicate_types.borrow().contains_key(&predicate_name) {
                unreachable!(); // some type has not been encoded before.
            }
            let ty = self.predicate_types.borrow()[&predicate_name];
            return self.encode_snapshot(&ty);
        }
        self.snapshots.borrow()[&predicate_name].clone()
    }

    pub fn get_snapshot(&self, snapshot_name: String) -> Box<Snapshot> {
        // fails if we have not encoded a snapshot with that name before
        // should be safe as we should never construct a snapshot name outside
        // of the snapshot encoder.
        let predicate_name = self.type_snapshots.borrow()[&snapshot_name].to_string();
        self.snapshots.borrow()[&predicate_name].clone()
    }

    pub fn encode_type_invariant_use(&self, ty: ty::Ty<'tcx>) -> String {
        // TODO we could use type_predicate_names instead (see TypeEncoder::encode_invariant_use)
        if !self.type_invariant_names.borrow().contains_key(ty.kind()) {
            let type_encoder = TypeEncoder::new(self, ty);
            let result = type_encoder.encode_invariant_use();
            self.type_invariant_names
                .borrow_mut()
                .insert(ty.kind().clone(), result);
            // Trigger encoding of definition
            self.encode_type_invariant_def(ty);
        }
        let invariant_name = self.type_invariant_names.borrow()[&ty.kind()].clone();
        invariant_name
    }

    pub fn encode_type_invariant_def(&self, ty: ty::Ty<'tcx>) -> vir::Function {
        let invariant_name = self.encode_type_invariant_use(ty);
        if !self.type_invariants.borrow().contains_key(&invariant_name) {
            let type_encoder = TypeEncoder::new(self, ty);
            let invariant = type_encoder.encode_invariant_def();
            self.type_invariants
                .borrow_mut()
                .insert(invariant_name.clone(), invariant);
        }
        self.type_invariants.borrow()[&invariant_name].clone()
    }

    pub fn encode_type_tag_use(&self, ty: ty::Ty<'tcx>) -> String {
        if !self.type_tag_names.borrow().contains_key(&ty.kind()) {
            let type_encoder = TypeEncoder::new(self, ty);
            let result = type_encoder.encode_tag_use();
            self.type_tag_names
                .borrow_mut()
                .insert(ty.kind().clone(), result);
            // Trigger encoding of definition
            self.encode_type_tag_def(ty);
        }
        let tag_name = self.type_tag_names.borrow()[&ty.kind()].clone();
        tag_name
    }

    pub fn encode_type_tag_def(&self, ty: ty::Ty<'tcx>) -> vir::Function {
        let tag_name = self.encode_type_tag_use(ty);
        if !self.type_tags.borrow().contains_key(&tag_name) {
            let type_encoder = TypeEncoder::new(self, ty);
            let tag = type_encoder.encode_tag_def();
            self.type_tags.borrow_mut().insert(tag_name.clone(), tag);
        }
        self.type_tags.borrow()[&tag_name].clone()
    }

    pub fn encode_const_expr(
        &self,
        ty: &ty::TyS<'tcx>,
        value: &ty::ConstKind<'tcx>
    ) -> vir::Expr {
        trace!("encode_const_expr {:?}", value);
        let scalar_value = match value {
            ty::ConstKind::Value(ref const_value) => {
                const_value
                    .try_to_scalar()
                    .expect(&format!("Unsupported const: {:?}", value))
            }
            ty::ConstKind::Unevaluated(def, substs, promoted) => {
                let tcx = self.env().tcx();
                let param_env = tcx.param_env(def.did);
                tcx
                    .const_eval_resolve(param_env, *def, substs, *promoted, None).ok()
                    .and_then(|const_value| const_value.try_to_scalar())
                    .expect(&format!("Unsupported const: {:?}", value))
            }
            _ => unimplemented!("{:?}", value),
        };

        fn with_sign(unsigned_val: u128, bit_size: u64) -> i128 {
            // Handle *signed* integers
            let shift = 128 - bit_size;
            let casted_val = unsigned_val as i128;
            // sign extend the raw representation to be an i128
            ((casted_val << shift) >> shift).into()
        }

        let expr = match ty.kind() {
            ty::TyKind::Bool => scalar_value.to_bool().unwrap().into(),
            ty::TyKind::Char => scalar_value.to_char().unwrap().into(),
            ty::TyKind::Int(ast::IntTy::I8) => scalar_value.to_i8().unwrap().into(),
            ty::TyKind::Int(ast::IntTy::I16) => scalar_value.to_i16().unwrap().into(),
            ty::TyKind::Int(ast::IntTy::I32) => scalar_value.to_i32().unwrap().into(),
            ty::TyKind::Int(ast::IntTy::I64) => scalar_value.to_i64().unwrap().into(),
            ty::TyKind::Int(ast::IntTy::I128) => {
                match scalar_value {
                    mir::interpret::Scalar::Raw { data, .. } => {
                        let val: i128 = with_sign(data, 128).try_into().unwrap();
                        val.into()
                    },
                    _ => unimplemented!(),
                }
            },
            ty::TyKind::Int(ast::IntTy::Isize) => {
                match scalar_value {
                    mir::interpret::Scalar::Raw { data, .. } => {
                        let isize_bits = mem::size_of::<isize>() * 8;
                        let val: isize = with_sign(data, isize_bits.try_into().unwrap()).try_into().unwrap();
                        val.into()
                    },
                    _ => unimplemented!(),
                }
            },
            ty::TyKind::Uint(ast::UintTy::U8) => scalar_value.to_u8().unwrap().into(),
            ty::TyKind::Uint(ast::UintTy::U16) => scalar_value.to_u16().unwrap().into(),
            ty::TyKind::Uint(ast::UintTy::U32) => scalar_value.to_u32().unwrap().into(),
            ty::TyKind::Uint(ast::UintTy::U64) => scalar_value.to_u64().unwrap().into(),
            ty::TyKind::Uint(ast::UintTy::U128) => {
                match scalar_value {
                    mir::interpret::Scalar::Raw { data, .. } => {
                        let val: u128 = data;
                        val.into()
                    },
                    _ => unimplemented!(),
                }
            },
            ty::TyKind::Uint(ast::UintTy::Usize) => {
                match scalar_value {
                    mir::interpret::Scalar::Raw { data, .. } => {
                        let val: usize = data.try_into().unwrap();
                        val.into()
                    },
                    _ => unimplemented!(),
                }
            }
            ref x => unimplemented!("{:?}", x),
        };
        debug!("encode_const_expr {:?} --> {:?}", value, expr);
        expr
    }

    pub fn encode_int_cast(&self, value: u128, ty: ty::Ty<'tcx>) -> vir::Expr {
        trace!("encode_int_cast {:?} as {:?}", value, ty);

        let expr = match ty.kind() {
            ty::TyKind::Bool => (value != 0).into(),
            ty::TyKind::Int(ast::IntTy::I8) => (value as i8).into(),
            ty::TyKind::Int(ast::IntTy::I16) => (value as i16).into(),
            ty::TyKind::Int(ast::IntTy::I32) => (value as i32).into(),
            ty::TyKind::Int(ast::IntTy::I64) => (value as i64).into(),
            ty::TyKind::Int(ast::IntTy::I128) => (value as i128).into(),
            ty::TyKind::Int(ast::IntTy::Isize) => (value as isize).into(),
            ty::TyKind::Uint(ast::UintTy::U8) => (value as u8).into(),
            ty::TyKind::Uint(ast::UintTy::U16) => (value as u16).into(),
            ty::TyKind::Uint(ast::UintTy::U32) => (value as u32).into(),
            ty::TyKind::Uint(ast::UintTy::U64) => (value as u64).into(),
            ty::TyKind::Uint(ast::UintTy::U128) => (value as u128).into(),
            ty::TyKind::Uint(ast::UintTy::Usize) => (value as usize).into(),
            ty::TyKind::Char => value.into(),
            ref x => unimplemented!("{:?}", x),
        };
        debug!("encode_int_cast {:?} as {:?} --> {:?}", value, ty, expr);
        expr
    }

    pub fn encode_item_name(&self, def_id: DefId) -> String {
        // Rule: the rhs must always have an even number of "$"
        let mut final_name = "m_".to_string();
        let name = if config::disable_name_mangling() {
            self.env.get_item_name(def_id)
        } else {
            self.env.get_item_def_path(def_id)
        };
        final_name.push_str(
            &name
                .replace("::", "$$")
                .replace("#", "$sharp$")
                .replace("<", "$openang$")
                .replace(">", "$closeang$")
                .replace("(", "$openrou$")
                .replace(")", "$closerou$")
                .replace("[", "$opensqu$")
                .replace("]", "$closesqu$")
                .replace("{", "$opencur$")
                .replace("}", "$closecur$")
                .replace(",", "$comma$")
                .replace(";", "$semic$")
                .replace(" ", "$space$"),
        );
        final_name
    }

    pub fn encode_invariant_func_app(&self, ty: ty::Ty<'tcx>, encoded_arg: vir::Expr) -> vir::Expr {
        let type_pred = self.encode_type_predicate_use(ty).unwrap(); // will panic if attempting to encode unsupported type
        vir::Expr::FuncApp(
            self.encode_type_invariant_use(ty),
            vec![encoded_arg],
            // TODO ?
            vec![vir::LocalVar::new("self", vir::Type::TypedRef(type_pred))],
            vir::Type::Bool,
            // TODO
            vir::Position::default(),
        )
    }

    pub fn encode_tag_func_app(&self, ty: ty::Ty<'tcx>) -> vir::Expr {
        vir::Expr::FuncApp(
            self.encode_type_tag_use(ty),
            vec![],
            // TODO ?
            vec![],
            vir::Type::Int,
            // TODO
            vir::Position::default(),
        )
    }

    /// Encode either a pure function body or a specification assertion (stored in the given MIR).
    pub fn encode_pure_function_body(&self, mut proc_def_id: ProcedureDefId) -> vir::Expr {
        proc_def_id = *self.get_specification_def_id(&proc_def_id);
        let substs_key = self.type_substitution_key();
        let key = (proc_def_id, substs_key);
        if !self.pure_function_bodies.borrow().contains_key(&key) {
            let procedure = self.env.get_procedure(proc_def_id);
            let pure_function_encoder = PureFunctionEncoder::new(
                self,
                proc_def_id,
                procedure.get_mir(),
                true,
            );
            let body = pure_function_encoder.encode_body();
            self.pure_function_bodies
                .borrow_mut()
                .insert(key.clone(), body);
        }
        self.pure_function_bodies.borrow()[&key].clone()
    }

    pub fn encode_pure_function_def(
        &self,
        mut proc_def_id: ProcedureDefId,
        substs: Vec<(ty::Ty<'tcx>, ty::Ty<'tcx>)>,
    ) {
        proc_def_id = *self.get_specification_def_id(&proc_def_id);
        trace!("[enter] encode_pure_function_def({:?})", proc_def_id);
        assert!(
            self.env.has_prusti_attribute(proc_def_id, "pure"),
            "procedure is not marked as pure: {:?}",
            proc_def_id
        );

        {
            // FIXME: this is a hack to support generics. See issue #187.
            let mut tymap_stack = self.typaram_repl.borrow_mut();
            assert!(tymap_stack.is_empty());
            let mut tymap = HashMap::new();
            for (typ, subst) in substs {
                tymap.insert(typ, subst);
            }
            tymap_stack.push(tymap);
        }

        // FIXME: Using substitutions as a key is most likely wrong.
        let substs_key = self.type_substitution_key();
        let key = (proc_def_id, substs_key);

        if !self.pure_functions.borrow().contains_key(&key) {
            trace!("not encoded: {:?}", key);
            let procedure = self.env.get_procedure(proc_def_id);
            let pure_function_encoder =
                PureFunctionEncoder::new(self, proc_def_id, procedure.get_mir(), false);
            let function = if self.is_trusted(proc_def_id) {
                pure_function_encoder.encode_bodyless_function()
            } else {
                let pure_function = pure_function_encoder.encode_function();
                self.patch_pure_post_with_mirror_call(pure_function)
            };

            self.log_vir_program_before_viper(function.to_string());
            self.pure_functions.borrow_mut().insert(key, function);
        }

        // FIXME: this is a hack to support generics. See issue #187.
        {
            let mut tymap_stack = self.typaram_repl.borrow_mut();
            tymap_stack.pop();
        }
        trace!("[exit] encode_pure_function_def({:?})", proc_def_id);
    }

    fn patch_pure_post_with_mirror_call(&self, function: vir::Function) -> vir::Function {
        // use function identifier to be more robust in the presence of generics
        let mirror = self.encode_pure_snapshot_mirror(
            function.get_identifier().to_string(),
            &function.formal_args,
            &function.return_type,
        );
        if mirror.is_none() {
            return function;
        }
        let mirror = mirror.unwrap();

        let mut mirror_args = vec![];
        for func_arg in &function.formal_args {
            let arg = vir::Expr::Local(func_arg.clone(), vir::Position::default());
            match &func_arg.typ {
                vir::Type::TypedRef(name) => {
                    mirror_args.push(
                        self.encode_snapshot_use(name.to_string())
                            .get_snap_call(arg),
                    );
                }
                _ => mirror_args.push(arg),
            }
        }

        let mut posts = function.posts.clone();
        posts.push(vir::Expr::InhaleExhale(
            box vir::Expr::BinOp(
                vir::BinOpKind::EqCmp,
                box vir::Expr::Local(
                    vir::LocalVar::new("__result", function.return_type.clone()),
                    vir::Position::default(),
                ),
                box vir::Expr::DomainFuncApp(
                    mirror.clone(),
                    mirror_args,
                    vir::Position::default(),
                ),
                    /* TODO
                    vir::Expr::DomainFuncApp(
                        mirror.name,
                        mirror_args,
                        mirror.formal_args,
                        mirror.return_type,
                        mirror.domain_name,
                        vir::Position::default(),
                    )
                     */
                vir::Position::default(),
            ),
            box vir::Expr::Const(vir::Const::Bool(true), vir::Position::default()),
            vir::Position::default()
        ));
        vir::Function { posts, ..function }
    }

    pub fn encode_pure_snapshot_mirror(&self,
                                       pure_func_name: String,
                                       pure_formal_args: &Vec<vir::LocalVar>,
                                       pure_return_type: &vir::Type)
                                       -> Option<vir::DomainFunc> {
        if !self.snap_mirror_funcs
            .borrow()
            .contains_key(&pure_func_name)
        {
            if !pure_formal_args.iter().all(
                |a| match &a.typ {
                    vir::Type::TypedRef(name) => {
                        self.encode_snapshot_use(name.to_string()).is_defined()
                    }
                    _ => true,
                }) {
                self.snap_mirror_funcs
                    .borrow_mut()
                    .insert(pure_func_name.to_string(), None);
            } else {
                let formal_args = pure_formal_args
                    .iter()
                    .map(|a| {
                        vir::LocalVar::new(
                            a.name.to_string(),
                            match &a.typ {
                                vir::Type::TypedRef(name) => {
                                    self.encode_snapshot_use(name.to_string()).get_type()
                                }
                                t => t.clone(),
                            },
                        )
                    })
                    .collect();

                let mirror_function = vir::DomainFunc {
                    name: format!("mirror${}", pure_func_name.to_string()),
                    formal_args,
                    return_type: pure_return_type.clone(),
                    unique: false,
                    domain_name: SNAPSHOT_MIRROR_DOMAIN.to_string(),
                };
                self.snap_mirror_funcs
                    .borrow_mut()
                    .insert(pure_func_name.to_string(), Some(mirror_function));
            }
        }
        self.snap_mirror_funcs.borrow()[&pure_func_name].clone()
    }

    pub fn get_item_name(&self, proc_def_id: ProcedureDefId) -> String {
        self.env.get_item_name(proc_def_id)
    }

    /// Encode the use (call) of a pure function, returning the name of the
    /// function and its type.
    ///
    /// The called function must be marked as pure.
    pub fn encode_pure_function_use(
        &self,
        mut proc_def_id: ProcedureDefId,
    ) -> (String, vir::Type) {
        proc_def_id = *self.get_specification_def_id(&proc_def_id);
        let procedure = self.env.get_procedure(proc_def_id);

        assert!(
            self.env.has_prusti_attribute(proc_def_id, "pure"),
            "procedure is not marked as pure: {:?}",
            proc_def_id
        );

        let pure_function_encoder =
            PureFunctionEncoder::new(self, proc_def_id, procedure.get_mir(), false);

        self.queue_pure_function_encoding(proc_def_id);

        // FIXME: encode_function_return_type assumes that pure functions cannot return generic values.
        (
            pure_function_encoder.encode_function_name(),
            pure_function_encoder.encode_function_return_type(),
        )
    }

    /// Encode the use (call) of either a comparison (equality or disequality)
    /// returning the name of the encoded function and its type.
    /// If the comparison is not supported, a stub function will be encoded
    pub fn encode_cmp_pure_function_use(
        &self,
        proc_def_id: ProcedureDefId,
        arg_ty: ty::Ty<'tcx>, // type arguments
        is_equality: bool // true = equality, false = disequality
    ) -> (String, vir::Type) {
        let snapshot = self.encode_snapshot(&arg_ty);
        if snapshot.is_defined() {
            (
                if is_equality {
                    snapshot.get_equals_func_name()
                } else {
                    snapshot.get_not_equals_func_name()
                },
                vir::Type::Bool
            )
        } else {
            self.encode_stub_pure_function_use(proc_def_id)
        }
    }

    /// Encode the use (call) of a stub pure function, returning the name of the
    /// function and its type.
    ///
    /// The stub function is a bodyless function with `false` precondition. It's meant to be used
    /// when the user tries to call an impure function in a context that requires a pure function.
    pub fn encode_stub_pure_function_use(
        &self,
        mut proc_def_id: ProcedureDefId,
    ) -> (String, vir::Type) {
        proc_def_id = *self.get_specification_def_id(&proc_def_id);
        // The stub function may come from another module for which we can have
        // only optimized_mir.
        let body = self.env.tcx().optimized_mir(proc_def_id);
        let encoder = StubFunctionEncoder::new(self, proc_def_id, body);

        // If we haven't seen this particular stub before, generate and insert it.
        let key = (proc_def_id, self.type_substitution_key());
        if !self.pure_functions.borrow().contains_key(&key) {
            let function = encoder.encode_function();

            self.log_vir_program_before_viper(function.to_string());

            self.stub_pure_functions.borrow_mut().insert(key, function);
        }
        (
            encoder.encode_function_name(),
            encoder.encode_function_return_type(),
        )
    }

    pub fn queue_procedure_encoding(&self, proc_def_id: ProcedureDefId) {
        self.encoding_queue
            .borrow_mut()
            .push((proc_def_id, Vec::new()));
    }

    pub fn queue_pure_function_encoding(&self, proc_def_id: ProcedureDefId) {
        let substs = self.current_tymap().into_iter().collect();
        self.encoding_queue.borrow_mut().push((proc_def_id, substs));
    }

    pub fn process_encoding_queue(&mut self) {
        self.initialize();
        while !self.encoding_queue.borrow().is_empty() {
            let (proc_def_id, substs) = self.encoding_queue.borrow_mut().pop().unwrap();
            let proc_name = self.env.get_absolute_item_name(proc_def_id);
            let proc_def_path = self.env.get_item_def_path(proc_def_id);
            let proc_span = self.env.get_item_span(proc_def_id);
            info!(
                "Encoding: {} from {:?} ({})",
                proc_name, proc_span, proc_def_path
            );
            let is_pure_function = self.env.has_prusti_attribute(proc_def_id, "pure");
            if is_pure_function {
                self.encode_pure_function_def(proc_def_id, substs);
            } else {
                assert!(substs.is_empty());
                if self.is_trusted(proc_def_id) {
                    debug!(
                        "Trusted procedure will not be encoded or verified: {:?}",
                        proc_def_id
                    );
                } else {
                    if let Err(error) = self.encode_procedure(proc_def_id) {
                        self.register_encoding_error(error);
                        debug!("Error encoding function: {:?}", proc_def_id);
                    }
                }
            }
        }
    }

    pub fn is_trusted(&self, def_id: ProcedureDefId) -> bool {
        trace!("is_trusted {:?}", def_id);
        let result = self.env().has_prusti_attribute(def_id, "trusted");
        trace!("is_trusted {:?} = {}", def_id, result);
        result
    }

    /// Convert a potential type parameter to a concrete type.
    pub fn resolve_typaram(&self, ty: ty::Ty<'tcx>) -> ty::Ty<'tcx> {
        // TODO: creating each time a current_tymap might be slow. This can be optimized.
        if let Some(replaced_ty) = self.current_tymap().get(&ty) {
            trace!("resolve_typaram({:?}) ==> {:?}", ty, replaced_ty);
            return replaced_ty
        }
        ty
    }

    /// Merges the stack of type maps into a single map.
    pub fn current_tymap(&self) -> HashMap<ty::Ty<'tcx>, ty::Ty<'tcx>> {
        let mut map = HashMap::new();
        for map_frame in self.typaram_repl.borrow().iter().rev() {
            for (typ, subst) in map_frame {
                map.insert(typ.clone(), subst.clone());
                let additional_substs: Vec<_> = map
                    .iter()
                    .filter(|(_typ1, typ2)| typ2 == &typ)
                    .map(|(typ1, typ2)| (typ1.clone(), typ2.clone()))
                    .collect();
                for (typ, subst) in additional_substs {
                    map.insert(typ, subst);
                }
            }
        }
        map
    }

    /// TODO: This is a hack, it generates strings that can be used to instantiate generic pure
    /// functions.
    pub fn type_substitution_strings(&self) -> HashMap<String, String> {
        self.current_tymap()
            .iter()
            .map(|(typ, subst)| {
                let encoded_typ = match self.encode_type(typ) {
                    vir::Type::TypedRef(s) => s.clone(),
                    x => unreachable!("{:?}", x),
                };
                let encoded_subst = match self.encode_type(subst) {
                    vir::Type::TypedRef(s) => s.clone(),
                    x => unreachable!("{:?}", x),
                };
                (encoded_typ, encoded_subst)
            })
            .collect()
    }

    /// TODO: This is a hack, it generates a String that can be used for uniquely identifying this
    /// type substitution.
    pub fn type_substitution_key(&self) -> String {
        let mut substs: Vec<_> = self
            .type_substitution_strings()
            .into_iter()
            .filter(|(typ, subst)| typ != subst)
            .map(|(typ, subst)| format!("({},{})", typ, subst))
            .collect();
        substs.sort();
        substs.join(";")
    }
}
