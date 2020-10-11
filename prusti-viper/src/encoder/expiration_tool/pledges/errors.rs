use std::collections::HashSet;

use prusti_interface::specs::typed;
use rustc_hir::ExprKind;
use rustc_span::Span;

use crate::encoder::errors::EncodingError;

use super::PledgeDependency;
use super::super::Context;

pub(super) fn unsupported_assertion(
    assertion: &typed::Assertion,
    spans: Vec<Span>
) -> EncodingError {
    let message = format!("assertions of type {:?} are not supported in pledges yet.",
        assertion.kind);
    EncodingError::incorrect(message, spans)
}

pub(super) fn unsupported_expression(expression: &ExprKind, span: Span) -> EncodingError {
    let message = format!("expressions of type {:?} are not supported in pledges yet.",
        expression);
    EncodingError::incorrect(message, span)
}

pub(super) fn before_expiry_contains_inputs(span: Span) -> EncodingError {
    let message = "the before_expiry environment cannot contain input references.";
    EncodingError::incorrect(message, span)
}

pub(super) fn after_unblocked_contains_outputs(span: Span) -> EncodingError {
    let message = "the after_unblocked environment cannot contain output references.";
    EncodingError::incorrect(message, span)
}

pub(super) fn ctxt_no_dependencies(context: Context, span: Span) -> EncodingError {
    let message = format!("this {} environment must contain at least one input or output \
        reference.", context);
    EncodingError::incorrect(message, span)
}

pub(super) fn ctxt_multiple_dependencies(
    context: Context,
    dependencies: &HashSet<PledgeDependency>
) -> EncodingError {
    let spans = dependencies.iter().map(|pd| pd.span).collect::<Vec<_>>();
    let message = format!("this {} environment has dependencies on multiple inputs or outputs.",
        context);
    EncodingError::incorrect(message, spans)
}

pub(super) fn ctxt_wrong_dependencies(
    context: Context,
    expected_dependency: &PledgeDependency,
    actual_dependency: &PledgeDependency
) -> EncodingError {
    let spans = vec![expected_dependency, actual_dependency].into_iter()
        .map(|pd| pd.span).collect::<Vec<_>>();
    let message = format!(
        "the actual dependencies of this {} environment don't match the expected dependency.",
        context);
    EncodingError::incorrect(message, spans)
}

pub(super) fn ctxt_wrong_expected_dependency(span: Span) -> EncodingError {
    let message = "this expected dependency is not of the right form.";
    EncodingError::incorrect(message, span)
}

pub(super) fn nested_environments(span: Span) -> EncodingError {
    let message = "cannot nest a before_expiry or after_unblocked environment inside another \
        before_expiry or after_unblocked environment.";
    EncodingError::incorrect(message, span)
}
