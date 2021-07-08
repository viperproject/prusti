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

struct SpecItem {
    spec_id: typed::SpecificationId,
    #[allow(dead_code)]
    spec_type: SpecType,
    specification: JsonAssertion,
}

impl fmt::Debug for SpecItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpecItem")
         .field("spec_id", &self.spec_id)
         .finish()
    }
}

struct ProcedureSpecRef {
    spec_id_refs: Vec<prusti_specs::specifications::common::SpecIdRef>,
    pure: bool,
    trusted: bool,
}

/// Specification collector, intended to be applied as a visitor over the crate
/// HIR. After the visit, [determine_def_specs] can be used to get back
/// a mapping of DefIds (which may not be local due to extern specs) to their
/// [SpecificationSet], i.e. procedures, loop invariants, and structs.
pub struct SpecCollector<'a, 'tcx: 'a> {
    tcx: TyCtxt<'tcx>,
    env: &'a Environment<'tcx>,
    extern_resolver: ExternSpecResolver<'tcx>,

    /// Collected assertions before deserialisation.
    spec_items: Vec<SpecItem>,

    typed_expressions: HashMap<String, LocalDefId>,

    /// Collected, deserialised assertions, keyed by their specification id.
    typed_specs: typed::SpecificationMap<'tcx>,

    /// Resolved specifications.
    procedure_specs: HashMap<LocalDefId, ProcedureSpecRef>,
    loop_specs: HashMap<LocalDefId, Vec<SpecificationId>>,
}

impl<'a, 'tcx> SpecCollector<'a, 'tcx> {
    pub fn new(env: &'a Environment<'tcx>) -> Self {
        Self {
            tcx: env.tcx(),
            env,
            spec_items: Vec::new(),
            typed_specs: HashMap::new(),
            procedure_specs: HashMap::new(),
            loop_specs: HashMap::new(),
            typed_expressions: HashMap::new(),
            extern_resolver: ExternSpecResolver::new(env.tcx()),
        }
    }

    fn prepare_typed_procedure_specs(&mut self) {
        let spec_items = std::mem::replace(&mut self.spec_items, vec![]);
        self.typed_specs = spec_items
            .into_iter()
            .map(|spec_item| {
                let assertion = reconstruct_typed_assertion(
                    spec_item.specification,
                    &self.typed_expressions,
                    self.env
                );
                (spec_item.spec_id, assertion)
            })
            .collect()
    }

    pub fn build_def_specs(mut self, env: &Environment<'tcx>) -> typed::DefSpecificationMap<'tcx> {
        self.prepare_typed_procedure_specs();

        let mut def_spec = typed::DefSpecificationMap::new();
        self.determine_procedure_specs(&mut def_spec);
        self.determine_extern_specs(&mut def_spec, env);
        self.determine_loop_specs(&mut def_spec);
        self.determine_struct_specs(&mut def_spec);
        def_spec
    }

    fn determine_extern_specs(&self, def_spec: &mut typed::DefSpecificationMap<'tcx>, env: &Environment<'tcx>) {
        self.extern_resolver.check_duplicates(env);
        // TODO: do something with the traits
        for (real_id, (_, spec_id)) in self.extern_resolver.extern_fn_map.iter() {
            if let Some(local_id) = real_id.as_local() {
                if def_spec.specs.contains_key(&local_id) {
                    PrustiError::incorrect(
                        format!("external specification provided for {}, which already has a specification",
                            env.get_item_name(*real_id)),
                        MultiSpan::from_span(env.get_item_span(*spec_id)),
                    ).emit(env);
                }
            }
            if let Some(_spec) = def_spec.specs.get(&spec_id.expect_local()) {
                def_spec.extern_specs.insert(*real_id, spec_id.expect_local());
            }
        }
    }

    fn determine_procedure_specs(&self, def_spec: &mut typed::DefSpecificationMap<'tcx>) {
        for (local_id, refs) in self.procedure_specs.iter() {
            let mut pres = Vec::new();
            let mut posts = Vec::new();
            let mut pledges = Vec::new();
            let mut predicate_body = None;
            for spec_id_ref in &refs.spec_id_refs {
                match spec_id_ref {
                    SpecIdRef::Precondition(spec_id) => {
                        pres.push(self.typed_specs.get(&spec_id).unwrap().clone());
                    }
                    SpecIdRef::Postcondition(spec_id) => {
                        posts.push(self.typed_specs.get(&spec_id).unwrap().clone());
                    }
                    SpecIdRef::Pledge{ lhs, rhs } => {
                        pledges.push(typed::Pledge {
                            reference: None,    // FIXME: Currently only `result` is supported.
                            lhs: lhs.map(|spec_id| self.typed_specs.get(&spec_id).unwrap().clone()),
                            rhs: self.typed_specs.get(&rhs).unwrap().clone(),
                        })
                    }
                    SpecIdRef::Predicate(spec_id) => {
                        predicate_body = Some(self.typed_specs.get(&spec_id).unwrap().clone());
                    }
                }
            }
            def_spec.specs.insert(
                *local_id,
                typed::SpecificationSet::Procedure(typed::ProcedureSpecification {
                    pres,
                    posts,
                    pledges,
                    predicate_body,
                    pure: refs.pure,
                    trusted: refs.trusted,
                })
            );
        }
    }

    fn determine_loop_specs(&self, def_spec: &mut typed::DefSpecificationMap<'tcx>) {
        for (local_id, spec_ids) in self.loop_specs.iter() {
            let specs = spec_ids.iter()
                .map(|spec_id| self.typed_specs.get(&spec_id).unwrap().clone())
                .collect();
            def_spec.specs.insert(*local_id, typed::SpecificationSet::Loop(typed::LoopSpecification {
                invariant: specs
            }));
        }
    }

    // TODO: struct specs
    fn determine_struct_specs(&self, _def_spec: &mut typed::DefSpecificationMap<'tcx>) {}
}

fn get_procedure_spec_ids(def_id: DefId, attrs: &[ast::Attribute]) -> Option<ProcedureSpecRef> {
    let mut spec_id_refs = vec![];

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
    spec_id_refs.extend(
        read_prusti_attr("pred_spec_id_ref", attrs).map(
            |raw_spec_id| SpecIdRef::Predicate(parse_spec_id(raw_spec_id))
        )
    );
    debug!("Function {:?} has specification ids {:?}", def_id, spec_id_refs);

    let pure = has_prusti_attr(attrs, "pure");
    let trusted = has_prusti_attr(attrs, "trusted");

    if pure || trusted || spec_id_refs.len() > 0 {
        Some(ProcedureSpecRef {
            spec_id_refs,
            pure,
            trusted,
        })
    } else {
        None
    }
}

fn reconstruct_typed_assertion<'tcx>(
    assertion: JsonAssertion,
    typed_expressions: &HashMap<String, LocalDefId>,
    env: &Environment<'tcx>
) -> typed::Assertion<'tcx> {
    assertion.to_typed(typed_expressions, env)
}

fn deserialize_spec_from_attrs(attrs: &[ast::Attribute]) -> JsonAssertion {
    let json_string = read_prusti_attr("assertion", attrs)
        .expect("could not find prusti::assertion");
    JsonAssertion::from_json_string(&json_string)
}

impl<'a, 'tcx> intravisit::Visitor<'tcx> for SpecCollector<'a, 'tcx> {
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

        // Collect a typed expression
        if let Some(expr_id) = read_prusti_attr("expr_id", attrs) {
            self.typed_expressions.insert(expr_id, local_id);
        }

        // Collect a specification id and its assertion
        if let Some(raw_spec_id) = read_prusti_attr("spec_id", attrs) {
            let spec_id: SpecificationId = raw_spec_id.try_into()
                .expect("failed conversion to SpecificationId");
            let specification = deserialize_spec_from_attrs(attrs);

            // Detect the kind of specification
            // FIXME: (minor) there is some redundancy here: the type of the
            // specification doesn't need to be checked. A procedure will refer
            // to its precondition with a #[pre_spec_id_ref=<id>] attribute,
            // where <id> is the unique identifier of the specification. Same
            // for postconditions and invariants.
            let spec_type = if has_prusti_attr(attrs, "loop_body_invariant_spec") {
                SpecType::Invariant
            } else {
                let fn_name = match fn_kind {
                    intravisit::FnKind::ItemFn(ref ident, ..) |
                    intravisit::FnKind::Method(ref ident, ..) => ident.name.to_ident_string(),
                    intravisit::FnKind::Closure => unreachable!(
                        "a closure is annotated with prusti::spec_id but not with \
                        prusti::loop_body_invariant_spec"
                    ),
                };
                if fn_name.starts_with("prusti_pre_item_")
                    || fn_name.starts_with("prusti_pre_closure_") {
                    SpecType::Precondition
                } else if fn_name.starts_with("prusti_post_item_")
                    || fn_name.starts_with("prusti_post_closure_") {
                    SpecType::Postcondition
                } else if fn_name.starts_with("prusti_pred_item_") {
                    SpecType::Predicate
                } else {
                    unreachable!()
                }
            };

            let spec_item = SpecItem {spec_id, spec_type, specification};
            self.spec_items.push(spec_item);

            // Collect loop invariant
            if spec_type == SpecType::Invariant {
                self.loop_specs
                    .entry(local_id)
                    .or_insert(vec![])
                    .push(spec_id);
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
