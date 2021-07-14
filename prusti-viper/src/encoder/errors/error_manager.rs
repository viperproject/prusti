// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use prusti_common::vir::Position;
use std::collections::HashMap;
use rustc_span::source_map::SourceMap;
use rustc_span::MultiSpan;
use viper::VerificationError;
use prusti_interface::PrustiError;
use log::debug;

/// The cause of a panic!()
#[derive(Clone, Debug)]
pub enum PanicCause {
    /// Generic cause
    Generic,
    /// Caused by a panic!()
    Panic,
    /// Caused by an assert!()
    Assert,
    /// Caused by an debug_assert!()
    DebugAssert,
    /// Caused by an unreachable!()
    Unreachable,
    /// Caused by an unimplemented!()
    Unimplemented,
}

/// In case of verification error, this enum will contain additional information
/// required to describe the error.
#[derive(Clone, Debug)]
pub enum ErrorCtxt {
    /// A Viper `assert false` that encodes a Rust panic
    Panic(PanicCause),
    /// A Viper `exhale expr` that encodes the call of a Rust procedure with precondition `expr`
    ExhaleMethodPrecondition,
    /// A Viper `assert expr` that encodes the call of a Rust procedure with precondition `expr`
    AssertMethodPostcondition,
    /// A Viper `assert expr` that encodes the call of a Rust procedure with precondition `expr`
    AssertMethodPostconditionTypeInvariants,
    /// A Viper `exhale expr` that encodes the end of a Rust procedure with postcondition `expr`
    ExhaleMethodPostcondition,
    /// A Viper `exhale expr` that exhales the permissions of a loop invariant `expr`
    ExhaleLoopInvariantOnEntry,
    ExhaleLoopInvariantAfterIteration,
    /// A Viper `assert expr` that asserts the functional specification of a loop invariant `expr`
    AssertLoopInvariantOnEntry,
    AssertLoopInvariantAfterIteration,
    /// A Viper `assert false` that encodes the failure (panic) of an `assert` Rust terminator
    /// Arguments: the message of the Rust assertion
    AssertTerminator(String),
    /// A Viper `assert false` in the context of a bounds check
    BoundsCheckAssert,
    /// A Viper `assert false` that encodes an `abort` Rust terminator
    AbortTerminator,
    /// A Viper `assert false` that encodes an `unreachable` Rust terminator
    #[allow(dead_code)]
    UnreachableTerminator,
    /// An error that should never happen
    Unexpected,
    /// A pure function definition
    #[allow(dead_code)]
    PureFunctionDefinition,
    /// A pure function call
    PureFunctionCall,
    /// An expression that encodes the value range of the result of a pure function
    PureFunctionPostconditionValueRangeOfResult,
    /// A Viper function with `false` precondition that encodes the failure (panic) of an
    /// `assert` Rust terminator in a Rust pure function.
    /// Arguments: the message of the Rust assertion
    PureFunctionAssertTerminator(String),
    /// A generic expression
    GenericExpression,
    /// A generic statement
    GenericStatement,
    /// Package a magic wand for the postcondition, at the end of a method
    PackageMagicWandForPostcondition,
    /// Apply a magic wand as a borrow expires, relevant for pledge conditions
    ApplyMagicWandOnExpiry,
    /// A diverging function call performed in a pure function
    DivergingCallInPureFunction,
    /// A Viper pure function call with `false` precondition that encodes a Rust panic in a pure function
    PanicInPureFunction(PanicCause),
    /// A Viper `assert e1 ==> e2` that encodes a weakening of the precondition
    /// of a method implementation of a trait
    AssertMethodPreconditionWeakening(MultiSpan),
    /// A Viper `assert e1 ==> e2` that encodes a strengthening of the precondition
    /// of a method implementation of a trait.
    AssertMethodPostconditionStrengthening(MultiSpan),
    /// A cast like `usize as u32`.
    TypeCast,
    /// A Viper `assert false` that encodes an unsupported feature
    Unsupported(String),
}

/// The error manager
#[derive(Clone)]
pub struct ErrorManager<'tcx> {
    codemap: &'tcx SourceMap,
    source_span: HashMap<u64, MultiSpan>,
    error_contexts: HashMap<u64, ErrorCtxt>,
    next_pos_id: u64,
}

impl<'tcx> ErrorManager<'tcx>
 {
    pub fn new(codemap: &'tcx SourceMap) -> Self {
        ErrorManager {
            codemap,
            source_span: HashMap::new(),
            error_contexts: HashMap::new(),
            next_pos_id: 1,
        }
    }

    pub fn register<T: Into<MultiSpan>>(&mut self, span: T, error_ctxt: ErrorCtxt) -> Position {
        let pos = self.register_span(span);
        self.register_error(&pos, error_ctxt);
        pos
    }

    pub fn register_span<T: Into<MultiSpan>>(&mut self, span: T) -> Position {
        let span = span.into();
        let pos_id = self.next_pos_id;
        self.next_pos_id += 1;
        debug!("Register position {:?} at span {:?}", pos_id, span);
        let pos = if let Some(primary_span) = span.primary_span() {
            let lines_info_res = self
                .codemap
                .span_to_lines(primary_span.source_callsite());
            if lines_info_res.is_err() {
                debug!("Error converting span to lines {:?}", lines_info_res.err());
                return Position::new(0, 0, pos_id.clone());
            }
            let lines_info = lines_info_res.unwrap();
            if let Some(first_line_info) = lines_info.lines.get(0) {
                let line = first_line_info.line_index as i32 + 1;
                let column = first_line_info.start_col.0 as i32 + 1;
                Position::new(line, column, pos_id.clone())
            } else {
                Position::new(0, 0, pos_id.clone())
            }
        } else {
            Position::new(0, 0, pos_id)
        };
        self.source_span.insert(pos_id, span);
        pos
    }

    pub fn register_error(&mut self, pos: &Position, error_ctxt: ErrorCtxt) {
        debug!("Register error at: {:?}", pos.id());
        self.error_contexts.insert(pos.id(), error_ctxt);
    }

    pub fn translate_verification_error(&self, ver_error: &VerificationError) -> PrustiError {
        debug!("Verification error: {:?}", ver_error);
        let opt_pos_id: Option<u64> = match ver_error.pos_id {
            Some(ref viper_pos_id) => {
                match viper_pos_id.parse() {
                    Ok(pos_id) => Some(pos_id),
                    Err(err) => {
                        return PrustiError::internal(
                            format!(
                                "unexpected Viper position '{}': {}",
                                viper_pos_id, err
                            ),
                            MultiSpan::new()
                        );
                    }
                }
            }
            None => None
        };
        let opt_reason_pos_id: Option<u64> = match ver_error.reason_pos_id {
            Some(ref viper_reason_pos_id) => {
                match viper_reason_pos_id.parse() {
                    Ok(reason_pos_id) => Some(reason_pos_id),
                    Err(err) => {
                        return PrustiError::internal(
                            format!(
                                "unexpected Viper reason position '{}': {}",
                                viper_reason_pos_id, err
                            ),
                            MultiSpan::new()
                        );
                    }
                }
            }
            None => None
        };

        let opt_error_ctxt = opt_pos_id.and_then(|pos_id| self.error_contexts.get(&pos_id));
        let opt_error_span = opt_pos_id.and_then(|pos_id| self.source_span.get(&pos_id));
        let opt_cause_span = opt_reason_pos_id.and_then(|reason_pos_id| {
            let res = self.source_span.get(&reason_pos_id);
            if res.is_none() {
                debug!("Unregistered reason position: {:?}", reason_pos_id);
            }
            res
        });

        let (error_span, error_ctxt) = if let Some(error_ctxt) = opt_error_ctxt {
            debug_assert!(opt_error_span.is_some());
            let error_span = opt_error_span.cloned().unwrap_or_else(|| MultiSpan::new());
            (error_span, error_ctxt)
        } else {
            debug!("Unregistered verification error: {:?}", ver_error);
            let error_span = if let Some(error_span) = opt_error_span {
                error_span.clone()
            } else {
                opt_cause_span.cloned().unwrap_or_else(|| MultiSpan::new())
            };

            match opt_pos_id {
                Some(ref pos_id) => {
                    return PrustiError::internal(
                        format!(
                            "unregistered verification error: [{}; {}] {}",
                            ver_error.full_id, pos_id, ver_error.message
                        ),
                        error_span
                    ).set_help(
                        "This could be caused by too small assertion timeout. \
                        Try increasing it by setting the configuration parameter \
                        ASSERT_TIMEOUT to a larger value."
                    )
                }
                None => {
                    return PrustiError::internal(
                        format!(
                            "unregistered verification error: [{}] {}",
                            ver_error.full_id, ver_error.message
                        ),
                        error_span
                    ).set_help(
                        "This could be caused by too small assertion timeout. \
                        Try increasing it by setting the configuration parameter \
                        ASSERT_TIMEOUT to a larger value."
                    )
                }
            }
        };

        match (ver_error.full_id.as_str(), error_ctxt) {
            ("assert.failed:assertion.false", ErrorCtxt::Panic(PanicCause::Generic)) => {
                PrustiError::verification("statement might panic", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::Panic(PanicCause::Panic)) => {
                PrustiError::verification("panic!(..) statement might be reachable", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::Panic(PanicCause::Assert)) |
            ("assert.failed:assertion.false", ErrorCtxt::Panic(PanicCause::DebugAssert)) => {
                    PrustiError::verification("the asserted expression might not hold", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::Panic(PanicCause::Unreachable)) => {
                PrustiError::verification("unreachable!(..) statement might be reachable", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::Panic(PanicCause::Unimplemented)) => {
                PrustiError::verification("unimplemented!(..) statement might be reachable", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::AssertTerminator(ref message)) => {
                PrustiError::verification(format!("assertion might fail with \"{}\"", message), error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::AbortTerminator) => {
                PrustiError::verification("statement might abort", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::UnreachableTerminator) => {
                PrustiError::internal(
                    "unreachable code might be reachable",
                    error_span
                ).set_failing_assertion(opt_cause_span)
                    .set_help("This might be a bug in the Rust compiler.")
            }

            ("assert.failed:assertion.false", ErrorCtxt::ExhaleMethodPrecondition) => {
                PrustiError::verification("precondition might not hold.", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("exhale.failed:assertion.false", ErrorCtxt::ExhaleMethodPrecondition) => {     //TODO: different error for failing exhale of type predicates?
                PrustiError::verification("precondition might not hold.", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("fold.failed:assertion.false", ErrorCtxt::ExhaleMethodPrecondition) => {
                PrustiError::verification(
                    "implicit type invariant expected by the function call might not hold.",
                    error_span
                ).set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::ExhaleMethodPostcondition) => {
                PrustiError::verification("postcondition might not hold.", error_span)
                    .push_primary_span(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::ExhaleLoopInvariantOnEntry) => {
                PrustiError::verification("loop invariant might not hold in the first loop iteration.", error_span)
                    .push_primary_span(opt_cause_span)
            }

            ("fold.failed:assertion.false", ErrorCtxt::ExhaleLoopInvariantOnEntry) => {
                PrustiError::verification(
                    "implicit type invariant of a variable might not hold on loop entry.",
                    error_span
                ).push_primary_span(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::AssertLoopInvariantOnEntry) => {
                PrustiError::verification("loop invariant might not hold in the first loop iteration.", error_span)
                    .push_primary_span(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::ExhaleLoopInvariantAfterIteration) => {
                PrustiError::verification(
                    "loop invariant might not hold after a loop iteration that preserves the loop condition.",
                    error_span
                ).push_primary_span(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::AssertLoopInvariantAfterIteration) => {
                PrustiError::verification(
                    "loop invariant might not hold after a loop iteration that preserves the loop condition.",
                    error_span
                ).push_primary_span(opt_cause_span)
            }

            ("application.precondition:assertion.false", ErrorCtxt::PureFunctionCall) => {
                PrustiError::verification(
                    "precondition of pure function call might not hold.",
                    error_span
                ).set_failing_assertion(opt_cause_span)
            }

            ("package.failed:assertion.false", ErrorCtxt::PackageMagicWandForPostcondition) => {
                PrustiError::verification(
                    "pledge in the postcondition might not hold.",
                    error_span
                ).push_primary_span(opt_cause_span)
            }

            (
                "application.precondition:assertion.false",
                ErrorCtxt::DivergingCallInPureFunction,
            ) => {
                PrustiError::verification(
                    "diverging function call in pure function might be reachable.",
                    error_span
                ).push_primary_span(opt_cause_span)
            }

            (
                "application.precondition:assertion.false",
                ErrorCtxt::PanicInPureFunction(PanicCause::Generic),
            ) => {
                PrustiError::verification("statement in pure function might panic", error_span)
                    .push_primary_span(opt_cause_span)
            }

            (
                "application.precondition:assertion.false",
                ErrorCtxt::PanicInPureFunction(PanicCause::Panic),
            ) => {
                PrustiError::verification(
                    "panic!(..) statement in pure function might panic",
                    error_span
                ).push_primary_span(opt_cause_span)
            }

            (
                "application.precondition:assertion.false",
                ErrorCtxt::PanicInPureFunction(PanicCause::Assert),
            ) => {
                PrustiError::verification("asserted expression might not hold", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            (
                "application.precondition:assertion.false",
                ErrorCtxt::PanicInPureFunction(PanicCause::Unreachable),
            ) => {
                PrustiError::verification(
                    "unreachable!(..) statement in pure function might be reachable",
                    error_span
                ).push_primary_span(opt_cause_span)
            }

            (
                "application.precondition:assertion.false",
                ErrorCtxt::PanicInPureFunction(PanicCause::Unimplemented),
            ) => {
                PrustiError::verification(
                    "unimplemented!(..) statement in pure function might be reachable",
                    error_span
                ).push_primary_span(opt_cause_span)
            }

            ("postcondition.violated:assertion.false", ErrorCtxt::PureFunctionDefinition) |
            ("postcondition.violated:assertion.false", ErrorCtxt::PureFunctionCall) |
            ("postcondition.violated:assertion.false", ErrorCtxt::GenericExpression) => {
                PrustiError::verification(
                    "postcondition of pure function definition might not hold",
                    error_span
                ).push_primary_span(opt_cause_span)
            }

            (
                "application.precondition:assertion.false",
                ErrorCtxt::PureFunctionAssertTerminator(ref message),
            ) => {
                PrustiError::verification(
                    format!("assertion might fail with \"{}\"", message),
                    error_span
                ).set_failing_assertion(opt_cause_span)
            },

            ("application.precondition:assertion.false", ErrorCtxt::TypeCast) => {
                PrustiError::verification(
                    "value might not fit into the target type.",
                    error_span
                ).set_failing_assertion(opt_cause_span)
            }

            ("apply.failed:assertion.false", ErrorCtxt::ApplyMagicWandOnExpiry) => {
                PrustiError::verification("obligation might not hold on borrow expiry", error_span)
                    .set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::AssertMethodPostcondition) => {
                PrustiError::verification(format!("postcondition might not hold."), error_span)
                    .push_primary_span(opt_cause_span)
            }

            (
                "assert.failed:assertion.false",
                ErrorCtxt::AssertMethodPostconditionTypeInvariants,
            ) => {
                PrustiError::verification(
                    format!("type invariants might not hold at the end of the method."),
                    error_span
                ).set_failing_assertion(opt_cause_span)
            },

            ("fold.failed:assertion.false", ErrorCtxt::PackageMagicWandForPostcondition) |
            ("fold.failed:assertion.false", ErrorCtxt::AssertMethodPostconditionTypeInvariants) => {
                PrustiError::verification(
                    format!("implicit type invariants might not hold at the end of the method."),
                    error_span
                ).set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::AssertMethodPreconditionWeakening(impl_span)) => {
                PrustiError::verification(format!("the method's precondition may not be a valid weakening of the trait's precondition."), error_span)
                    //.push_primary_span(opt_cause_span)
                    .push_primary_span(Some(&impl_span))
                    .set_help("The trait's precondition should imply the implemented method's precondition.")
            }

            ("assert.failed:assertion.false", ErrorCtxt::AssertMethodPostconditionStrengthening(impl_span)) => {
                PrustiError::verification(format!("the method's postcondition may not be a valid strengthening of the trait's postcondition."), error_span)
                    //.push_primary_span(opt_cause_span)
                    .push_primary_span(Some(&impl_span))
                    .set_help("The implemented method's postcondition should imply the trait's postcondition.")
            }

            ("assert.failed:assertion.false", ErrorCtxt::BoundsCheckAssert) |
            ("application.precondition:assertion.false", ErrorCtxt::BoundsCheckAssert) => {
                PrustiError::verification(
                    "the array or slice index may be out of bounds".to_string(),
                    error_span,
                ).set_failing_assertion(opt_cause_span)
            }

            ("assert.failed:assertion.false", ErrorCtxt::Unsupported(ref reason)) => {
                PrustiError::unsupported(
                    format!("an unsupported Rust feature might be reachable: {}.", reason),
                    error_span
                ).set_failing_assertion(opt_cause_span)
            }

            (full_err_id, ErrorCtxt::Unexpected) => {
                PrustiError::internal(
                    format!(
                        "unexpected verification error: [{}] {}",
                        full_err_id, ver_error.message
                    ),
                    error_span,
                ).set_failing_assertion(
                    opt_cause_span
                ).set_help(
                    "This could be caused by too small assertion timeout. \
                    Try increasing it by setting the configuration parameter \
                    ASSERT_TIMEOUT to a larger value."
                )
            },

            (full_err_id, _) => {
                debug!(
                    "Unhandled verification error: {:?}, context: {:?}",
                    ver_error, error_ctxt
                );
                PrustiError::internal(
                    format!(
                        "unhandled verification error: {:?} [{}] {}",
                        error_ctxt, full_err_id, ver_error.message,
                    ),
                    error_span,
                ).set_failing_assertion(
                    opt_cause_span
                ).set_help(
                    "This could be caused by too small assertion timeout. \
                    Try increasing it by setting the configuration parameter \
                    ASSERT_TIMEOUT to a larger value."
                )
            }
        }
    }
}
