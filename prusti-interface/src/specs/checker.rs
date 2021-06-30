use log::{debug, info};
use rustc_hir::{
    self as hir,
    def_id::{DefId, LocalDefId},
    intravisit::{self, Visitor},
    itemlikevisit::ItemLikeVisitor,
};
use rustc_middle::{hir::map::Map, ty::TyCtxt};
use rustc_span::{MultiSpan, Span};

use std::collections::HashMap;

use crate::{
    environment::Environment,
    utils::{has_prusti_attr, has_spec_only_attr},
    PrustiError,
};

/// Checker visitor for the specifications. Currently checks that `predicate!`
/// functions are never used from non-specification code, but more checks may follow.
pub struct SpecChecker {
    /// Map of the `DefID`s to the `Span`s of `predicate!` functions found in the first pass.
    predicates: HashMap<DefId, Span>,

    /// Span of use and definition of predicates used outside of specifications, collected in the second pass.
    pred_usages: Vec<(Span, Span)>,
}

/// First predicate checks visitor: collect all function items that originate
/// from predicates
struct CollectPredicatesVisitor<'v, 'tcx> {
    tcx: TyCtxt<'tcx>,

    predicates: &'v mut HashMap<DefId, Span>,
}

impl<'v, 'tcx> intravisit::Visitor<'tcx> for CollectPredicatesVisitor<'v, 'tcx> {
    type Map = Map<'tcx>;

    fn nested_visit_map(&mut self) -> intravisit::NestedVisitorMap<Self::Map> {
        intravisit::NestedVisitorMap::All(self.tcx.hir())
    }

    fn visit_fn(
        &mut self,
        fk: intravisit::FnKind<'tcx>,
        fd: &'tcx hir::FnDecl<'tcx>,
        b: hir::BodyId,
        s: Span,
        id: hir::HirId,
    ) {
        // collect this fn's DefId if predicate function
        let attrs = self.tcx.hir().attrs(id);
        if has_prusti_attr(attrs, "pred_spec_id_ref") {
            let def_id = self.tcx.hir().local_def_id(id).to_def_id();
            self.predicates.insert(def_id, s);
        }

        intravisit::walk_fn(self, fk, fd, b, s, id);
    }
}

/// Second predicate checks visitor: check any references to predicate functions
/// from non-specification code
struct CheckPredicatesVisitor<'v, 'tcx> {
    tcx: TyCtxt<'tcx>,

    predicates: &'v HashMap<DefId, Span>,
    pred_usages: &'v mut Vec<(Span, Span)>,
}

impl<'v, 'tcx> Visitor<'tcx> for CheckPredicatesVisitor<'v, 'tcx> {
    type Map = Map<'tcx>;

    fn nested_visit_map(&mut self) -> intravisit::NestedVisitorMap<Self::Map> {
        intravisit::NestedVisitorMap::All(self.tcx.hir())
    }

    fn visit_item(&mut self, i: &'tcx hir::Item<'tcx>) {
        // restrict to "interesting" sub-nodes to visit, i.e. anything that
        // could be or contain call (or other usage of predicate) expressions
        use hir::ItemKind::*;

        match i.kind {
            Static(_, _, _) | Fn(_, _, _) | Mod(_) | Impl { .. } => {
                intravisit::walk_item(self, i);
            }
            _ => {
                // don't recurse into e.g. struct decls, type aliases, consts etc.
            }
        }
    }

    fn visit_expr(&mut self, ex: &'tcx hir::Expr<'tcx>) {
        if let hir::ExprKind::Path(ref path) = ex.kind {
            let res = self.tcx.typeck(ex.hir_id.owner).qpath_res(path, ex.hir_id);
            if let hir::def::Res::Def(_, def_id) = res {
                if let Some(pred_def_span) = self.predicates.get(&def_id) {
                    self.pred_usages.push((ex.span, *pred_def_span));
                }
            }
        }

        intravisit::walk_expr(self, ex);
    }

    fn visit_fn(
        &mut self,
        fk: intravisit::FnKind<'tcx>,
        fd: &'tcx hir::FnDecl<'tcx>,
        b: hir::BodyId,
        s: Span,
        id: hir::HirId,
    ) {
        // Stop checking inside `prusti::spec_only` functions
        let attrs = self.tcx.hir().attrs(id);
        if has_spec_only_attr(attrs) {
            return;
        }

        intravisit::walk_fn(self, fk, fd, b, s, id);
    }
}

impl<'tcx> SpecChecker {
    pub fn new() -> Self {
        Self {
            predicates: HashMap::new(),
            pred_usages: Vec::new(),
        }
    }

    pub fn check_predicate_usages(&mut self, tcx: TyCtxt<'tcx>, krate: &'tcx hir::Crate<'tcx>) {
        let mut collect = CollectPredicatesVisitor {
            tcx,
            predicates: &mut self.predicates,
        };
        intravisit::walk_crate(&mut collect, krate);

        let mut visit = CheckPredicatesVisitor {
            tcx: collect.tcx,
            predicates: &self.predicates,
            pred_usages: &mut self.pred_usages,
        };
        intravisit::walk_crate(&mut visit, krate);

        debug!("Predicate funcs: {:?}", self.predicates);
        debug!("Predicate usages: {:?}", self.pred_usages);
    }

    pub fn report_errors(&self, env: &Environment<'tcx>) {
        for &(usage_span, def_span) in &self.pred_usages {
            PrustiError::incorrect(
                "using predicate from non-specification code is not allowed".to_string(),
                MultiSpan::from_span(usage_span),
            )
            .add_note("this is a specification-only predicate function", Some(def_span))
            .emit(env);
        }
    }
}
