use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::once;
use std::str::FromStr;

use itertools::Itertools;

use prusti_interface::environment::mir_utils::PlaceAddProjection;
use prusti_interface::specs::typed;
use prusti_interface::specs::typed::Spanned;
use prusti_specs::specifications::common::AssertionKind;
use rustc_ast::ast::LitKind;
use rustc_hir::ExprKind;
use rustc_hir::Guard;
use rustc_hir::QPath;
use rustc_middle::hir;
use rustc_middle::mir;
use rustc_middle::ty;
use rustc_span::symbol::Symbol;

use crate::encoder::interface_reborrowing_graph::InterfaceReborrowingGraph;
use crate::encoder::places;
use crate::encoder::procedure_encoder::Result;

use super::errors;
use super::PledgeDependency;
use super::super::Context;

/// Analyzes the assertion and determines all return places that appear inside a `before_expiry`
/// environment and input places that appear inside an `after_unblocked` environment. The result
/// is a list of `PledgeDependency` structs, each of which describes one `before_expiry` or
/// `after_unblocked` instance.
pub(in super::super) fn identify_dependencies<'tcx>(
    tcx: ty::TyCtxt<'tcx>,
    mir: &mir::Body<'tcx>,
    reborrow_signature: &InterfaceReborrowingGraph<places::Place<'tcx>>,
    assertion: &typed::Assertion<'tcx>
) -> Result<HashSet<PledgeDependency<'tcx>>> {
    // Construct a mapping from places to their names in the original program.
    let local_names = mir.var_debug_info.iter()
        .filter_map(|vdi| Some((vdi.place.as_local()?, vdi.name)))
        .collect::<HashMap<_, _>>();

    // Gather all MIR arguments that are references and are blocked by something, together with
    // their original name name in the source program.
    let inputs = reborrow_signature.blocked.iter()
        .map(|place| {
            let local = &original_place(place).local;
            let name = local_names[local];
            (name, place)
        })
        .collect::<HashMap<_, _>>();

    // Gather all outputs that are references and are blocking something.
    let outputs = reborrow_signature.blocking.iter().collect::<HashSet<_>>();

    let identifier = DependencyIdentifier { tcx, mir, inputs: &inputs, outputs: &outputs };
    identifier.analyze_assertion(assertion)
}

fn original_place<'a, 'tcx>(place: &'a places::Place<'tcx>) -> &'a mir::Place<'tcx> {
    match place {
        places::Place::NormalPlace(place) | places::Place::SubstitutedPlace { place, .. } => place,
    }
}

struct DependencyIdentifier<'v, 'tcx> {
    tcx: ty::TyCtxt<'tcx>,
    mir: &'v mir::Body<'tcx>,
    inputs: &'v HashMap<Symbol, &'v places::Place<'tcx>>,
    outputs: &'v HashSet<&'v places::Place<'tcx>>,
}

impl<'v, 'tcx> DependencyIdentifier<'v, 'tcx> {
    /// See documentation of [identify_dependencies](fn.identify_dependencies.html).
    fn analyze_assertion(&self,
        assertion: &typed::Assertion<'tcx>,
    ) -> Result<HashSet<PledgeDependency<'tcx>>> {
        Ok(match assertion.kind.as_ref() {
            AssertionKind::Expr(expression) => {
                let expression = extract_expression(self.tcx.hir(), expression);
                self.analyze_expression(&expression.kind, expression.span, None)?
            },
            AssertionKind::And(assertions) =>
                assertions.iter()
                    .map(|assertion| self.analyze_assertion(assertion))
                    .collect::<Result<Vec<_>>>()?.into_iter().flatten().collect(),
            AssertionKind::Implies(antecedent, consequent) =>
                std::iter::empty()
                    .chain(self.analyze_assertion(antecedent)?)
                    .chain(self.analyze_assertion(consequent)?)
                    .collect(),
            _ => {
                let spans = assertion.get_spans(self.mir, self.tcx);
                Err(errors::unsupported_assertion(assertion, spans))?
            }
        })
    }

    /// See documentation of [identify_dependencies](fn.identify_dependencies.html).
    fn analyze_expression(&self,
        expression: &ExprKind,
        span: rustc_span::Span,
        context: Option<Context>
    ) -> Result<HashSet<PledgeDependency<'tcx>>> {
        let before_expiry_dependencies =
            self.determine_before_expiry_dependencies(expression, span);
        let after_unblocked_dependencies =
            self.determine_after_unblocked_dependencies(expression, span);

        if let Some(context) = context {
            if context == Context::AfterUnblocked && !before_expiry_dependencies.is_empty() {
                Err(errors::after_unblocked_contains_outputs(span))?;
            }
            if context == Context::BeforeExpiry && !after_unblocked_dependencies.is_empty() {
                Err(errors::before_expiry_contains_inputs(span))?;
            }
        }

        let recursive_dependencies = match expression {
            ExprKind::Binary(_, left, right) =>
                [left, right].iter()
                    .map(|expr| self.analyze_expression(&expr.kind, expr.span, context))
                    .collect::<Result<Vec<_>>>()?.into_iter().flatten().collect(),
            ExprKind::Unary(_, expression) =>
                self.analyze_expression(&expression.kind, expression.span, context)?,
            ExprKind::Call(function, arguments) =>
                self.analyze_call(function, arguments, span, context)?,
            ExprKind::MethodCall(_, _, arguments, _) =>
                arguments.iter()
                    .map(|arg| self.analyze_expression(&arg.kind, arg.span, context))
                    .collect::<Result<Vec<_>>>()?.into_iter().flatten().collect::<HashSet<_>>(),
            ExprKind::Field(base, _) =>
                self.analyze_expression(&base.kind, base.span, context)?,
            ExprKind::Match(matched, arms, _) => {
                let arm_expressions = arms.into_iter()
                    .flat_map(|arm| if let Some(Guard::If(guard)) = arm.guard {
                        vec![guard, arm.body]
                    } else {
                        vec![arm.body]
                    });
                once(matched as &rustc_hir::Expr).chain(arm_expressions)
                    .map(|expr| self.analyze_expression(&expr.kind, expr.span, context))
                    .collect::<Result<Vec<_>>>()?.into_iter().flatten().collect()
            }
            ExprKind::DropTemps(expression) =>
                self.analyze_expression(&expression.kind, expression.span, context)?,
            ExprKind::Block(block, _) => {
                assert!(block.stmts.is_empty());
                let expression = block.expr.as_ref().unwrap();
                self.analyze_expression(&expression.kind, expression.span, context)?
            }
            ExprKind::Path(_) => HashSet::new(),
            ExprKind::Lit(_) => HashSet::new(),
            _ => Err(errors::unsupported_expression(expression, span))?
        };

        let dependencies = std::iter::empty()
            .chain(before_expiry_dependencies)
            .chain(after_unblocked_dependencies)
            .chain(recursive_dependencies)
            .collect();

        Ok(dependencies)
    }

    fn analyze_call(&self,
        function: &rustc_hir::Expr,
        arguments: &[rustc_hir::Expr],
        span: rustc_span::Span,
        context: Option<Context>,
    ) -> Result<HashSet<PledgeDependency<'tcx>>> {
        let function_name = fully_qualified_name(&function);
        let (new_context, expected_dependency, arguments) = self.update_context(
            context, &function_name, arguments, span)?;
        let dependencies = arguments.iter()
            .map(|arg| self.analyze_expression(&arg.kind, arg.span, new_context))
            .collect::<Result<Vec<_>>>()?.into_iter().flatten().collect::<HashSet<_>>();
        if let (Some(new_context), None) = (new_context, context) {
            // We just entered a before_expiry or after_unblocked context, so we check if
            // it contains the right number of dependencies (i.e., exactly one).
            if dependencies.is_empty() {
                // A before_expiry or after_unblocked context cannot contain no
                // dependencies.
                Err(errors::ctxt_no_dependencies(new_context, span))
            } else if dependencies.len() > 1 {
                // A before_expiry or after_unblocked context cannot contain multiple
                // dependencies.
                Err(errors::ctxt_multiple_dependencies(new_context, &dependencies))
            } else {
                if let Some(expected_dependency) = &expected_dependency {
                    let actual_dependency = dependencies.iter().next().unwrap();
                    if actual_dependency != expected_dependency {
                        Err(errors::ctxt_wrong_dependencies(
                            new_context, &expected_dependency, &actual_dependency))?
                    }
                }
                Ok(dependencies)
            }
        } else { Ok(dependencies) }
    }

    /// A function call can change the context for the argument expressions, if the function called
    /// is the the special before_expiry or after_unblocked environment. This function implements
    /// this logic based on the name of the called function. It fails if the called function is one
    /// of the two special environments and the current context is not `None`, because this would
    /// mean a before_expiry or after_unblocked environment is nested inside another before_expiry or
    /// after_unblocked environment, which is illegal.
    ///
    /// It returns the context that is valid for the arguments of the function call as the first
    /// tuple element.
    ///
    /// If the called function is either `before_expiry` or `after_unblocked` and the reference
    /// that needs to expire or needs to be unblocked is stated explicitly, the second tuple
    /// element holds corresponding the pledge dependency.
    ///
    /// The third tuple element holds the arguments that should be probed further for dependencies.
    /// Usually, this is just the `arguments` slice passed into `update_context`. If the called
    /// function is either `before_expiry` or `after_unblocked`, the first argument is dropped,
    /// because it is used to specify the expected dependency and does not constitute a dependency
    /// in itself.
    fn update_context<'hir, 'args>(&self,
        current_context: Option<Context>,
        function_name: &str,
        arguments: &'args [rustc_hir::Expr<'hir>],
        span: rustc_span::Span,
    ) -> Result<(
        Option<Context>,
        Option<PledgeDependency>,
        &'args [rustc_hir::Expr<'hir>]
    )> {
        Ok(match function_name {
            "before_expiry" => {
                assert_context_is_none(&current_context, span)?;
                let expected_dependency = self.analyze_expected_dependency(
                    &arguments[0].kind, arguments[0].span)?;
                (Some(Context::BeforeExpiry), expected_dependency, &arguments[1..])
            },
            "after_unblocked" => {
                assert_context_is_none(&current_context, span)?;
                let expected_dependency = self.analyze_expected_dependency(
                    &arguments[0].kind, arguments[0].span)?;
                (Some(Context::AfterUnblocked), expected_dependency, &arguments[1..])
            },
            _ => (current_context, None, arguments)
        })
    }

    fn analyze_expected_dependency(&self,
        expected_dependency: &ExprKind,
        span: rustc_span::Span,
    ) -> Result<Option<PledgeDependency>> {
        Ok(match expected_dependency {
            ExprKind::AddrOf(_, _, expression) => {
                let expected_dependencies = self.determine_before_expiry_dependencies(
                    &expression.kind, expression.span);
                if expected_dependencies.len() != 1 {
                    Err(errors::ctxt_wrong_expected_dependency(span))?
                } else {
                    expected_dependencies.into_iter().next()
                }
            },
            ExprKind::Lit(lit) => match &lit.node {
                LitKind::Int(0, _) => None,
                _ => Err(errors::ctxt_wrong_expected_dependency(span))?,
            },
            _ => Err(errors::ctxt_wrong_expected_dependency(span))?,
        })
    }

    /// Returns all pledge dependencies on output references that are immediately implied by this
    /// expression, by checking which output references match this expression.
    fn determine_before_expiry_dependencies(&self,
        expression: &ExprKind,
        span: rustc_span::Span,
    ) -> HashSet<PledgeDependency<'tcx>> {
        self.outputs.iter().filter_map(|&output| {
            let original_output = original_place(output);
            let original_output = original_output.truncate(self.tcx, 1);
            if expr_matches_place(expression, &original_output) {
                Some(PledgeDependency {
                    context: Context::BeforeExpiry,
                    place: output.clone(),
                    span
                })
            } else { None }
        }).collect()
    }

    /// Returns all pledge dependencies on input references that are immediately implied by this
    /// expression, by checking which input references match this expression.
    fn determine_after_unblocked_dependencies(&self,
        expression: &ExprKind,
        span: rustc_span::Span
    ) -> HashSet<PledgeDependency<'tcx>> {
        if let ExprKind::Path(QPath::Resolved(_, path)) = expression {
            let segments = &path.segments;
            if segments.len() == 1 {
                let name = segments[0].ident.name;
                if let Some(&place) = self.inputs.get(&name) {
                    once(PledgeDependency {
                        context: Context::AfterUnblocked,
                        place: place.clone(),
                        span
                    }).collect()
                } else { HashSet::new() }
            } else { HashSet::new() }
        } else { HashSet::new() }
    }
}

/// Given a `typed::Expression`, this method utters some incantations to conjure up the HIR
/// expression representing this `typed::Expression`.
fn extract_expression<'hir>(
    hir: hir::map::Map<'hir>,
    expression: &typed::Expression
) -> &'hir rustc_hir::Expr<'hir> {
    // This is the id of the closure that contains the expression.
    let hir_id = hir.local_def_id_to_hir_id(expression.expr);

    // This is the closure.
    let closure = hir.expect_expr(hir_id);
    let closure_body_id = match &closure.kind {
        rustc_hir::ExprKind::Closure(_, _, body, _, _) => body.hir_id,
        _ => unreachable!()
    };

    // This is the body of the closure
    let body = hir.expect_expr(closure_body_id);
    match &body.kind {
        rustc_hir::ExprKind::Block(block, _) => {
            assert!(block.stmts.is_empty());
            block.expr.unwrap()
        },
        _ => unreachable!()
    }
}

/// Returns true if the given HIR expression corresponds to the given MIR place. This is not
/// complete by far. The expression and place must be a series of field projections. They must
/// also be based on the return place (i.e., `result` for the HIR expression and `_0` for the MIR
/// place).
fn expr_matches_place<'tcx>(expr: &ExprKind, place: &mir::Place<'tcx>) -> bool {
    // We only support the return place.
    if place.local.index() != 0 {
        return false;
    }

    // Turn the expression into a variable and path of fields accessed starting from this variable.
    let (variable, path) = if let Some(result) = expr_to_variable_and_path(expr) {
        result
    } else {
        // The expression is not of a shape that we support.
        return false;
    };

    // We only support the return place.
    if variable.as_str() != "result" {
        return false;
    }

    // If they don't access the same number of fields, they cannot denote the same place.
    if path.len() != place.projection.len() {
        return false;
    }

    // Now that we know that both the HIR expression and the MIR place start at the return place
    // and access the same number of fields, check that they also agree in the list of fields
    // they access.
    place.projection.iter().zip(path.iter()).all(|(place_elem, path_elem)|
        if let mir::ProjectionElem::Field(field, _) = place_elem {
            usize::from_str(&path_elem.as_str()) == Ok(field.index())
        } else {
            // We only support field projections.
            false
        })
}

/// Converts an expression of the form `variable.field1.field2.field3` into the list of symbols
/// [`identifier`, `field1`, `field2`, `field3`].
///
/// If the expression is not of this form, it returns `None`.
fn expr_to_variable_and_path(expr: &ExprKind) -> Option<(Symbol, Vec<Symbol>)> {
    match expr {
        ExprKind::Field(base, ident) => {
            let (variable, mut base_path) = expr_to_variable_and_path(&base.kind)?;
            base_path.push(ident.name);
            Some((variable, base_path))
        }
        ExprKind::Path(QPath::Resolved(_, path)) => {
            let segments = &path.segments;
            if segments.len() == 1 {
                Some((segments[0].ident.name, Vec::new()))
            } else { None }
        }
        _ => None
    }
}

/// If this expression is a path like `seg1::seg2::seg3`, it returns the string representation of
/// it. If this expression is not a path, it panics.
fn fully_qualified_name(function: &rustc_hir::Expr) -> String {
    match &function.kind {
        ExprKind::Path(QPath::Resolved(_, path)) =>
            path.segments.iter()
                .map(|segment| segment.ident.name.to_string())
                .join("::"),
        _ => unreachable!()
    }
}

fn assert_context_is_none(
    context: &Option<Context>,
    span: rustc_span::Span,
) -> Result<()> {
    if context.is_some() { Err(errors::nested_environments(span)) }
    else { Ok(()) }
}
