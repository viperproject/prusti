use prusti_specs::specifications::{json::Assertion as JsonAssertion, SpecType};
use rustc_ast::ast;
use rustc_hir::{intravisit, ItemKind};
use rustc_middle::hir::map::Map;
use rustc_middle::ty::TyCtxt;
use rustc_span::{Span, MultiSpan};
use rustc_span::symbol::Symbol;
use rustc_hir::def_id::{DefId, LocalDefId};
use std::collections::HashMap;
use std::convert::TryInto;
use crate::environment::Environment;
use crate::PrustiError;
use crate::utils::{
    has_spec_only_attr, has_extern_spec_attr, read_prusti_attr, read_prusti_attrs, has_prusti_attr
};
use log::debug;

pub mod external;
pub mod typed;
pub mod checker;

use typed::StructuralToTyped;
use typed::SpecIdRef;
use std::fmt;
use crate::specs::external::ExternSpecResolver;
use prusti_specs::specifications::common::SpecificationId;

#[derive(Debug)]
struct ProcedureSpecRefs {
    spec_id_refs: Vec<prusti_specs::specifications::common::SpecIdRef>,
    pure: bool,
    trusted: bool,
}

/// Specification collector, intended to be applied as a visitor over the crate
/// HIR. After the visit, [determine_def_specs] can be used to get back
/// a mapping of DefIds (which may not be local due to extern specs) to their
/// [SpecificationSet], i.e. procedures, loop invariants, and structs.
pub struct SpecCollector<'tcx> {
    tcx: TyCtxt<'tcx>,
    extern_resolver: ExternSpecResolver<'tcx>,

    /// Map between specification ids and their typed expression (inside the function)
    spec_functions: HashMap<SpecificationId, LocalDefId>,

    /// Map between specified function/loop and its specifications
    procedure_specs: HashMap<LocalDefId, ProcedureSpecRefs>,
    loop_specs: HashMap<LocalDefId, Vec<SpecificationId>>,
}

impl<'tcx> SpecCollector<'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self {
            tcx: tcx,
            extern_resolver: ExternSpecResolver::new(tcx),

            spec_functions: HashMap::new(),
            
            procedure_specs: HashMap::new(),
            loop_specs: HashMap::new(),
        }
    }

    pub fn build_def_specs(mut self, env: &Environment<'tcx>) -> typed::DefSpecificationMap<'tcx> {
        println!("DEBUG: spec_functions: {:#?}", self.spec_functions);
        println!("DEBUG: procedure_specs: {:#?}", self.procedure_specs);
        println!("DEBUG: loop_specs: {:#?}", self.loop_specs);

        typed::DefSpecificationMap::new() // TODO
    }
}

fn parse_spec_id(spec_id: String, def_id: DefId) -> SpecificationId {
    spec_id.try_into().expect(
        &format!("cannot parse the spec_id attached to {:?}", def_id)
    )
}

fn get_procedure_spec_ids(def_id: DefId, attrs: &[ast::Attribute]) -> Option<ProcedureSpecRefs> {
    let mut spec_id_refs = vec![];

    spec_id_refs.extend(
        read_prusti_attrs("pre_spec_id_ref", attrs).into_iter().map(
            |raw_spec_id| SpecIdRef::Precondition(parse_spec_id(raw_spec_id, def_id))
        )
    );
    spec_id_refs.extend(
        read_prusti_attrs("post_spec_id_ref", attrs).into_iter().map(
            |raw_spec_id| SpecIdRef::Postcondition(parse_spec_id(raw_spec_id, def_id))
        )
    );
    spec_id_refs.extend(
        read_prusti_attrs("pledge_spec_id_ref", attrs).into_iter().map(
            |raw_spec_id| SpecIdRef::Pledge(parse_spec_id(raw_spec_id, def_id))
        )
    );
    spec_id_refs.extend(
        read_prusti_attr("pred_spec_id_ref", attrs).map(
            |raw_spec_id| SpecIdRef::Predicate(parse_spec_id(raw_spec_id, def_id))
        )
    );
    debug!("Function {:?} has specification ids {:?}", def_id, spec_id_refs);

    let pure = has_prusti_attr(attrs, "pure");
    let trusted = has_prusti_attr(attrs, "trusted");

    if pure || trusted || spec_id_refs.len() > 0 {
        Some(ProcedureSpecRefs {
            spec_id_refs,
            pure,
            trusted,
        })
    } else {
        None
    }
}

impl<'tcx> intravisit::Visitor<'tcx> for SpecCollector<'tcx> {
    type Map = Map<'tcx>;

    fn nested_visit_map(&mut self) -> intravisit::NestedVisitorMap<Self::Map> {
        let map = self.tcx.hir();
        intravisit::NestedVisitorMap::All(map)
    }

    fn visit_trait_item(
        &mut self,
        ti: &'tcx rustc_hir::TraitItem,
    ) {
        intravisit::walk_trait_item(self, ti);

        let id = ti.hir_id();
        let local_id = self.tcx.hir().local_def_id(id);
        let def_id = local_id.to_def_id();
        let attrs = self.tcx.get_attrs(ti.def_id.to_def_id());

        // Collect procedure specifications
        if let Some(procedure_spec_ref) = get_procedure_spec_ids(def_id, attrs) {
            self.procedure_specs.insert(local_id, procedure_spec_ref);
        }
    }

    fn visit_fn(
        &mut self,
        fn_kind: intravisit::FnKind<'tcx>,
        fn_decl: &'tcx rustc_hir::FnDecl,
        body_id: rustc_hir::BodyId,
        span: Span,
        id: rustc_hir::hir_id::HirId,
    ) {
        intravisit::walk_fn(self, fn_kind, fn_decl, body_id, span, id);

        let local_id = self.tcx.hir().local_def_id(id);
        let def_id = local_id.to_def_id();
        let attrs = self.tcx.hir().attrs(id);

        // Collect external function specifications
        if has_extern_spec_attr(attrs) {
            self.extern_resolver.add_extern_fn(fn_kind, fn_decl, body_id, span, id);
        }

        // Collect procedure specifications
        if let Some(procedure_spec_ref) = get_procedure_spec_ids(def_id, attrs) {
            self.procedure_specs.insert(local_id, procedure_spec_ref);
        }

        // Collect spec functions
        if let Some(raw_spec_id) = read_prusti_attr("spec_id", attrs) {
            let spec_id: SpecificationId = parse_spec_id(raw_spec_id, def_id);
            self.spec_functions.insert(spec_id, local_id);

            // Collect loop specifications            
            if has_prusti_attr(attrs, "loop_body_invariant_spec") {
                self.loop_specs.entry(local_id).or_insert(vec![]).push(spec_id);
            }
        }
    }

    fn visit_stmt(
        &mut self,
        stmt: &'tcx rustc_hir::Stmt,
    ) {
        intravisit::walk_stmt(self, stmt);

        // Collect closure specifications
        if let rustc_hir::StmtKind::Local(local) = stmt.kind {
            let attrs = self.tcx.hir().attrs(local.hir_id);
            if has_prusti_attr(attrs, "closure") {
                let init_expr = local.init
                    .expect("closure on Local without assignment");
                let local_id = self.tcx.hir().local_def_id(init_expr.hir_id);
                let def_id = local_id.to_def_id();
                // Collect procedure specifications
                if let Some(procedure_spec_ref) = get_procedure_spec_ids(def_id, attrs) {
                    self.procedure_specs.insert(local_id, procedure_spec_ref);
                }
            }
        }
    }
}
