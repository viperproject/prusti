use prusti_specs::specifications::common;
use prusti_specs::specifications::json;
use rustc_hir::BodyId;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_middle::{mir, ty::{self, TyCtxt}};
use rustc_span::Span;
use std::collections::HashMap;

pub use common::{ExpressionId, SpecType, SpecificationId, SpecIdRef};
use crate::data::ProcedureDefId;
use crate::environment::Environment;

// FIXME: these comments are not terribly useful and are a copy of the untyped ones...
/// A specification that has no types associated with it.
pub type Specification<'tcx> = common::Specification<ExpressionId, LocalDefId, (mir::Local, ty::Ty<'tcx>)>;
/// A set of untyped specifications associated with a single element.
pub type SpecificationSet<'tcx> = common::SpecificationSet<ExpressionId, LocalDefId, (mir::Local, ty::Ty<'tcx>)>;
/// A set of untyped specifications associated with a loop.
pub type LoopSpecification<'tcx> = common::LoopSpecification<ExpressionId, LocalDefId, (mir::Local, ty::Ty<'tcx>)>;
/// A set of untyped specifications associated with a procedure.
pub type ProcedureSpecification<'tcx> = common::ProcedureSpecification<ExpressionId, LocalDefId, (mir::Local, ty::Ty<'tcx>)>;
/// A map of untyped specifications for a specific crate.
pub type SpecificationMap<'tcx> = HashMap<common::SpecificationId, Assertion<'tcx>>;
/// An assertion that has no types associated with it.
pub type Assertion<'tcx> = common::Assertion<ExpressionId, LocalDefId, (mir::Local, ty::Ty<'tcx>)>;
/// An assertion kind that has no types associated with it.
pub type AssertionKind<'tcx> = common::AssertionKind<ExpressionId, LocalDefId, (mir::Local, ty::Ty<'tcx>)>;
/// An expression that has no types associated with it.
pub type Expression = common::Expression<ExpressionId, LocalDefId>;
/// A trigger set that has no types associated with it.
pub type TriggerSet = common::TriggerSet<ExpressionId, LocalDefId>;
/// For all variables that have no types associated with it.
pub type ForAllVars<'tcx> = common::ForAllVars<ExpressionId, (mir::Local, ty::Ty<'tcx>)>;
/// Specification entailment variables that have no types associated.
pub type SpecEntailmentVars<'tcx> = common::SpecEntailmentVars<ExpressionId, (mir::Local, ty::Ty<'tcx>)>;
/// A trigger that has no types associated with it.
pub type Trigger = common::Trigger<ExpressionId, LocalDefId>;
/// A pledge in the postcondition.
pub type Pledge<'tcx> = common::Pledge<ExpressionId, LocalDefId, (mir::Local, ty::Ty<'tcx>)>;

/// A map of specifications keyed by crate-local DefIds.
pub struct DefSpecificationMap<'tcx> {
    pub specs: HashMap<LocalDefId, SpecificationSet<'tcx>>,
    pub extern_specs: HashMap<DefId, LocalDefId>,
}

impl<'tcx> DefSpecificationMap<'tcx> {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
            extern_specs: HashMap::new(),
        }
    }
    pub fn get(&self, def_id: &DefId) -> Option<&SpecificationSet<'tcx>> {
        let id = if let Some(spec_id) = self.extern_specs.get(def_id) {
            *spec_id
        } else {
            def_id.as_local()?
        };
        self.specs.get(&id)
    }
}

/// This trait is implemented for specification-related types that have one or
/// more associated spans (positions within the source code). The spans are not
/// necessarily contiguous, and may be used for diagnostic reporting.
pub trait Spanned<'tcx> {
    /// Returns the spans for the given value. `mir` is the function body used
    /// to resolve positions of `rustc_middle::mir::Local` indices, `tcx` is
    /// used to resolve positions of global items.
    fn get_spans(&self, mir_body: &mir::Body<'tcx>, tcx: TyCtxt<'tcx>) -> Vec<Span>;
}

impl<'tcx> Spanned<'tcx> for Expression {
    fn get_spans(&self, _mir_body: &mir::Body<'tcx>, tcx: TyCtxt<'tcx>) -> Vec<Span> {
        vec![tcx.def_span(self.expr)]
    }
}

impl<'tcx> Spanned<'tcx> for ForAllVars<'tcx> {
    fn get_spans(&self, mir_body: &mir::Body<'tcx>, _tcx: TyCtxt<'tcx>) -> Vec<Span> {
        self.vars
            .iter()
            .filter_map(|v| mir_body.local_decls.get(v.0))
            .map(|v| v.source_info.span)
            .collect()
    }
}

impl<'tcx> Spanned<'tcx> for Assertion<'tcx> {
    fn get_spans(&self, mir_body: &mir::Body<'tcx>, tcx: TyCtxt<'tcx>) -> Vec<Span> {
        match *self.kind {
            AssertionKind::Expr(ref assertion_expr) => assertion_expr.get_spans(mir_body, tcx),
            AssertionKind::And(ref assertions) => {
                assertions
                    .iter()
                    .flat_map(|a| a.get_spans(mir_body, tcx))
                    .collect()
            }
            AssertionKind::Implies(ref lhs, ref rhs) => {
                let mut spans = lhs.get_spans(mir_body, tcx);
                spans.extend(rhs.get_spans(mir_body, tcx));
                spans
            }
            AssertionKind::ForAll(ref vars, ref trigger_set, ref body) => {
                let mut spans = vars.get_spans(mir_body, tcx);
                spans.extend(trigger_set
                    .triggers()
                    .iter()
                    .flat_map(|t| t.terms())
                    .flat_map(|e| e.get_spans(mir_body, tcx))
                    .collect::<Vec<Span>>());
                spans.extend(body.get_spans(mir_body, tcx));
                spans
            }
            AssertionKind::TypeCond(ref vars, ref body) => {
                let mut spans = vars.get_spans(mir_body, tcx);
                spans.extend(body.get_spans(mir_body, tcx));
                spans
            }
            AssertionKind::SpecEntailment {
                ref closure,
                ref pres,
                ref posts,
                ..
            } => {
                let mut spans = closure.get_spans(mir_body, tcx);
                spans.extend(pres
                    .iter()
                    .flat_map(|pre| pre.get_spans(mir_body, tcx))
                    .collect::<Vec<Span>>());
                spans.extend(posts
                    .iter()
                    .flat_map(|post| post.get_spans(mir_body, tcx))
                    .collect::<Vec<Span>>());
                spans
            }
        }
    }
}

pub trait StructuralToTyped<'tcx, Target> {
    fn to_typed(
        self,
        typed_expressions: &HashMap<String, LocalDefId>,
        env: &Environment<'tcx>,
    ) -> Target;
}

impl<'tcx> StructuralToTyped<'tcx, Expression> for json::Expression {
    fn to_typed(self, typed_expressions: &HashMap<String, LocalDefId>, _env: &Environment<'tcx>) -> Expression {
        let local_id = typed_expressions[&format!("{}_{}", self.spec_id, self.expr_id)];
        Expression {
            spec_id: self.spec_id,
            id: self.expr_id,
            expr: local_id,
        }
    }
}

impl<'tcx> StructuralToTyped<'tcx, TriggerSet> for json::TriggerSet {
    fn to_typed(self, typed_expressions: &HashMap<String, LocalDefId>, env: &Environment<'tcx>) -> TriggerSet {
        common::TriggerSet(
            self.0
                .into_iter()
                .map(|x| x.to_typed(typed_expressions, env))
                .collect()
        )
    }
}

impl<'tcx> StructuralToTyped<'tcx, Trigger> for json::Trigger {
    fn to_typed(self, typed_expressions: &HashMap<String, LocalDefId>, env: &Environment<'tcx>) -> Trigger {
        common::Trigger(
            self.0
                .into_iter()
                .map(|x| x.to_typed(typed_expressions, env))
                .collect()
        )
    }
}

impl<'tcx> StructuralToTyped<'tcx, ForAllVars<'tcx>> for json::ForAllVars {
    fn to_typed(
        self,
        typed_expressions: &HashMap<String, LocalDefId>,
        env: &Environment<'tcx>,
    ) -> ForAllVars<'tcx> {
        let local_id = typed_expressions[&format!("{}_{}", self.spec_id, self.expr_id)];
        let body = env.local_mir(local_id);

        // the first argument to the node is the closure itself and the
        // following ones are the variables; therefore, we need to skip
        // the first one
        let vars: Vec<(mir::Local, ty::Ty)> = body
            .args_iter()
            .skip(1)
            .map(|arg| (arg, body.local_decls
                           .get(arg)
                           .unwrap()
                           .ty))
            .collect();

        assert!(body.arg_count-1 == self.count);
        assert_eq!(vars.len(), self.count);
        return ForAllVars {
            spec_id: self.spec_id,
            id: self.expr_id,
            vars
        }
    }
}

impl<'tcx> StructuralToTyped<'tcx, SpecEntailmentVars<'tcx>> for json::SpecEntailmentVars {
    fn to_typed(
        self,
        typed_expressions: &HashMap<String, LocalDefId>,
        env: &Environment<'tcx>
    ) -> SpecEntailmentVars<'tcx> {
        let local_pre_id = typed_expressions[&format!("{}_{}", self.spec_id, self.pre_expr_id)];
        let local_post_id = typed_expressions[&format!("{}_{}", self.spec_id, self.post_expr_id)];
        let pre_body = env.local_mir(local_pre_id);
        let post_body = env.local_mir(local_post_id);

        let pre_args: Vec<(mir::Local, ty::Ty)> = pre_body
            .args_iter()
            .skip(1)
            .map(|arg| (arg, pre_body.local_decls
                                     .get(arg)
                                     .unwrap()
                                     .ty))
            .collect();
        let post_args: Vec<(mir::Local, ty::Ty)> = post_body
            .args_iter()
            .skip(1)
            .map(|arg| (arg, post_body.local_decls
                                      .get(arg)
                                      .unwrap()
                                      .ty))
            .collect();

        assert!(pre_body.arg_count - 1 == self.arg_count);
        assert!(post_body.arg_count - 1 == self.arg_count + 1); // arguments + "result"
        assert_eq!(pre_args.len(), self.arg_count);
        assert_eq!(post_args.len(), self.arg_count + 1);
        return SpecEntailmentVars {
            spec_id: self.spec_id,
            pre_id: self.pre_expr_id,
            post_id: self.post_expr_id,
            args: pre_args,
            result: *post_args.last().unwrap()
        }
    }
}

impl<'tcx> StructuralToTyped<'tcx, AssertionKind<'tcx>> for json::AssertionKind {
    fn to_typed(self, typed_expressions: &HashMap<String, LocalDefId>, env: &Environment<'tcx>) -> AssertionKind<'tcx> {
        use json::AssertionKind::*;
        match self {
            Expr(expr) => AssertionKind::Expr(expr.to_typed(typed_expressions, env)),
            And(assertions) => AssertionKind::And(
                assertions.into_iter()
                          .map(|assertion| assertion.to_typed(typed_expressions, env))
                          .collect()
            ),
            Implies(lhs, rhs) => AssertionKind::Implies(
                lhs.to_typed(typed_expressions, env),
                rhs.to_typed(typed_expressions, env)
            ),
            ForAll(vars, body, triggers) => AssertionKind::ForAll(
                vars.to_typed(typed_expressions, env),
                triggers.to_typed(typed_expressions, env),
                body.to_typed(typed_expressions, env),
            ),
            SpecEntailment {closure, arg_binders, pres, posts} => AssertionKind::SpecEntailment {
                closure: closure.to_typed(typed_expressions, env),
                arg_binders: arg_binders.to_typed(typed_expressions, env),
                pres: pres.into_iter()
                    .map(|pre| pre.to_typed(typed_expressions, env))
                    .collect(),
                posts: posts.into_iter()
                    .map(|post| post.to_typed(typed_expressions, env))
                    .collect(),
            },
        }
    }
}

impl<'tcx> StructuralToTyped<'tcx, Assertion<'tcx>> for json::Assertion {
    fn to_typed(self, typed_expressions: &HashMap<String, LocalDefId>, env: &Environment<'tcx>) -> Assertion<'tcx> {
        Assertion {
            kind: box self.kind.to_typed(typed_expressions, env),
        }
    }
}
