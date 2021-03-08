// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::encoder::errors::{
    ErrorCtxt, SpannedEncodingResult, SpannedEncodingError, EncodingError, WithSpan
};
use crate::encoder::mir_encoder::{MirEncoder, PlaceEncoder, PlaceEncoding};
use crate::encoder::mir_encoder::PRECONDITION_LABEL;
use crate::encoder::mir_interpreter::{
    run_backward_interpretation_point_to_point, BackwardMirInterpreter,
    MultiExprBackwardInterpreterState,
};
use crate::encoder::pure_function_encoder::PureFunctionBackwardInterpreter;
use crate::encoder::Encoder;
use prusti_common::config;
use crate::encoder::SpecFunctionKind;
use prusti_common::vir;
use prusti_common::vir::ExprIterator;
use prusti_interface::specs::typed;
use rustc_hir as hir;
use rustc_hir::def_id::DefId;
use rustc_middle::mir;
use rustc_middle::ty;
use std::collections::HashMap;
use rustc_ast::ast;
use log::{debug, trace};
use prusti_interface::utils::read_prusti_attr;

/// Encode an assertion coming from a specification to a `vir::Expr`.
///
/// In this documentation, we distinguish the encoding of a _value_ of a Rust expression from
/// the encoding of its _memory location_. For example, when encoding non-pure code:
/// * given an argument `x: u32` the Viper encoding will use `x: Ref` to encode the memory
///   location and `x.val_int: Int` to encode the value;
/// * given an argument `y: &u32` the Viper encoding will use `y: Ref` to encode the memory
///   location and `y.val_ref: Ref` to encode the value.
///
/// Arguments:
/// * `encoder`: a reference to the `Encoder`.
/// * `assertion`: the assertion to be encoded.
/// * `pre_label`: the label to be used to encode `old(..)` expressions. This should be `None` iff
///   the assertion cannot have old expressions (e.g. a precondition).
/// * `target_args`: the expression to be used to encode arguments.
/// * `target_return`: the expression to be used to encode `return` expressions. This should be
///   `None` iff the assertion cannot mention `return` (e.g. a loop invariant).
/// * `targets_are_values`: if `true`, the elements of `target_args` and `target_return` encode
///   _values_ and not _memory locations_. This is typically used to encode pure functions.
/// * `assertion_location`: the basic block at which the assertion should be encoded. This should
///   be `Some(..)` iff the assertion is a loop invariant.
pub fn encode_spec_assertion<'v, 'tcx: 'v>(
    encoder: &Encoder<'v, 'tcx>,
    assertion: &typed::Assertion<'tcx>,
    pre_label: Option<&str>,
    target_args: &[vir::Expr],
    target_return: Option<&vir::Expr>,
    targets_are_values: bool,
    assertion_location: Option<mir::BasicBlock>,
    parent_def_id: DefId,
) -> SpannedEncodingResult<vir::Expr> {
    let spec_encoder = SpecEncoder::new(
        encoder,
        pre_label.unwrap_or(""),
        target_args,
        target_return,
        targets_are_values,
        assertion_location,
        parent_def_id,
    );
    spec_encoder.encode_assertion(assertion)
}

struct SpecEncoder<'p, 'v: 'p, 'tcx: 'v> {
    encoder: &'p Encoder<'v, 'tcx>,
    /// The label to encode `old(..)` expressions
    pre_label: &'p str,
    /// The expression that encodes the arguments.
    target_args: &'p [vir::Expr],
    /// The expression that encodes `return` in a postcondition.
    target_return: Option<&'p vir::Expr>,
    /// Used to encode pure functions.
    targets_are_values: bool,
    /// Location at which to encode loop invariants.
    assertion_location: Option<mir::BasicBlock>,
    /// When registering errors, this gives us their
    /// associated function
    parent_def_id: DefId,
}

impl<'p, 'v: 'p, 'tcx: 'v> SpecEncoder<'p, 'v, 'tcx> {
    fn new(
        encoder: &'p Encoder<'v, 'tcx>,
        pre_label: &'p str,
        target_args: &'p [vir::Expr],
        target_return: Option<&'p vir::Expr>,
        targets_are_values: bool,
        assertion_location: Option<mir::BasicBlock>,
        parent_def_id: DefId,
    ) -> Self {
        trace!("SpecEncoder constructor");

        SpecEncoder {
            encoder,
            pre_label,
            target_args,
            target_return,
            targets_are_values,
            assertion_location,
            parent_def_id,
        }
    }

    /// Encode a quantified variable `arg`, given its type `arg_ty` and a unique identifier
    /// `forall_id` of the quantification.
    fn encode_forall_arg(
        &self,
        arg: mir::Local,
        arg_ty: ty::Ty<'tcx>,
        forall_id: &str
    ) -> vir::LocalVar {
        trace!("encode_forall_arg: {:?} {:?} {:?}", arg, arg_ty, forall_id);
        let ty = self.encoder.encode_type(arg_ty).unwrap();
        // FIXME: find a reasonable position when using this function below,
        // change return to EncodingResult<...>, then unwrap with ?
        let var_name = format!("{:?}_forall_{}", arg, forall_id);
        vir::LocalVar::new(var_name, ty)
    }

    fn encode_trigger(
        &self,
        trigger: &typed::Trigger,
        bounded_vars: &[vir::LocalVar]
    ) -> SpannedEncodingResult<vir::Trigger> {
        trace!("encode_trigger {:?}", trigger);
        struct TriggerChecker {
            span: rustc_span::MultiSpan,
            error: Option<SpannedEncodingError>,
        }
        impl vir::ExprWalker for TriggerChecker {
            fn walk(&mut self, expr: &vir::Expr) {
                match expr {
                    vir::Expr::Local(..) |
                    vir::Expr::Const(..) |
                    vir::Expr::FuncApp(..) |
                    vir::Expr::DomainFuncApp(..) => {
                        // Legal triggers.
                    }
                    _ => {
                        // Everything else is illegal in triggers.
                        let msg = "Only function calls are allowed in triggers.";
                        // TODO: We should use a more precise span.
                        self.error = Some(SpannedEncodingError::incorrect(msg, self.span.clone()));
                    }
                }
                if self.error.is_none() {
                    vir::default_walk_expr(self, expr);
                }
            }
        }
        let bounded_vars: Vec<_> = bounded_vars.iter().map(|var| var.clone().into()).collect();
        let mut found_bounded_vars = std::collections::HashSet::new();
        let mut encoded_expressions = Vec::new();
        for term in trigger.terms() {
            let encoded_expr = self.encode_expression(term)?;
            let mut checker = TriggerChecker {
                error: None,
                span: self.encoder.env().tcx().def_span(term.expr).into(),
            };
            vir::ExprWalker::walk(&mut checker, &encoded_expr);
            if let Some(error) = checker.error {
                return Err(error);
            }
            for var in &bounded_vars {
                if encoded_expr.find(var) {
                    found_bounded_vars.insert(var);
                }
            }
            encoded_expressions.push(encoded_expr);
        }
        for var in &bounded_vars {
            if !found_bounded_vars.contains(var) {
                // FIXME: We should mention the missing variable in the error message.
                let msg = "A trigger must mention all bounded variables.";
                let span = rustc_span::MultiSpan::from_spans(
                    trigger.terms().iter().map(|term| self.encoder.env().tcx().def_span(term.expr)).collect()
                );
                return Err(SpannedEncodingError::incorrect(msg, span));
            }
        }
        Ok(vir::Trigger::new(encoded_expressions))
    }

    /// Encode a specification item as a single expression.
    fn encode_assertion(&self, assertion: &typed::Assertion<'tcx>)
        -> SpannedEncodingResult<vir::Expr>
    {
        trace!("encode_assertion {:?}", assertion);
        Ok(match assertion.kind {
            box typed::AssertionKind::Expr(ref assertion_expr) =>
                self.encode_expression(assertion_expr)?,
            box typed::AssertionKind::And(ref assertions) => assertions
                .iter()
                .map(|x| self.encode_assertion(x))
                .collect::<Result<Vec<vir::Expr>, _>>()?
                .into_iter()
                .conjoin(),
            box typed::AssertionKind::Implies(ref lhs, ref rhs) => {
                vir::Expr::implies(
                    self.encode_assertion(lhs)?,
                    self.encode_assertion(rhs)?
                )
            }
            box typed::AssertionKind::TypeCond(ref vars, ref assertion) => {
                let enc = |ty: ty::Ty<'tcx>| -> vir::Expr {
                    // FIXME: this is a hack to support generics. See issue #187.
                    let ty = self.encoder.resolve_typaram(ty);
                    self.encoder.encode_tag_func_app(ty)
                };
                let typecond =
                    vir::Expr::eq_cmp(enc(vars.vars[0].1), enc(vars.vars[1].1));
                vir::Expr::implies(
                    typecond,
                    self.encode_assertion(assertion)?
                )
            }
            box typed::AssertionKind::ForAll(ref vars, ref trigger_set, ref body) => {
                let mut encoded_args = Vec::new();
                let mut bounds = Vec::new();
                for (arg, ty) in &vars.vars {
                    // TODO: how to get a span for the variable here?
                    //if !self.encoder.is_quantifiable(ty).unwrap() {
                    //    return Err(EncodingError::incorrect(
                    //        "This type cannot be used in quantifiers.",
                    //    ).with_span(?));
                    //}

                    let encoded_arg = self.encode_forall_arg(*arg, ty, &format!("{}_{}", vars.spec_id, vars.id));
                    if config::check_overflows() {
                        bounds.extend(self.encoder.encode_type_bounds(&encoded_arg.clone().into(), ty));
                    } else if config::encode_unsigned_num_constraint() {
                        if let ty::TyKind::Uint(_) = ty.kind() {
                            let expr = vir::Expr::le_cmp(0.into(), encoded_arg.clone().into());
                            bounds.push(expr);
                        }
                    }
                    encoded_args.push(encoded_arg);
                }
                let mut encoded_triggers = Vec::new();
                for trigger in trigger_set.triggers() {
                    let encoded_trigger = self.encode_trigger(trigger, &encoded_args)?;
                    encoded_triggers.push(encoded_trigger);
                }
                let encoded_body = self.encode_assertion(body)?;
                let final_body = if bounds.is_empty() {
                    encoded_body
                } else {
                    vir::Expr::implies(bounds.into_iter().conjoin(), encoded_body)
                };
                vir::Expr::forall(
                    encoded_args,
                    encoded_triggers,
                    final_body,
                )
            },
            box typed::AssertionKind::SpecEntailment {
                ref closure,
                arg_binders: ref vars,
                ref pres,
                ref posts,
            } => {
                // TODO: refactor, simplify, or extract into a function
                let tcx = self.encoder.env().tcx();
                let mir = self.encoder.env().local_mir(closure.expr);
                let result = &mir.local_decls[(0 as u32).into()];
                let ty = result.ty;
                if let Some(ty_repl) = self.encoder.current_tymap().get(ty) {
                    debug!("spec ent repl: {:?} -> {:?}", ty, ty_repl);

                    match ty_repl.kind() {
                        ty::TyKind::Closure(def_id, _substs)
                        | ty::TyKind::FnDef(def_id, _substs) => {
                            let encoded_pres = pres.iter()
                                .map(|x| self.encode_assertion(x))
                                .collect::<Result<Vec<vir::Expr>, _>>()?
                                .into_iter()
                                .conjoin();

                            // encode_forall_arg() above only works for integers.
                            // Therefore, for the time being, check that we're working with integers:
                            vars.args.iter().for_each(|(_arg, arg_ty)| {
                                match arg_ty.kind() {
                                    ty::TyKind::Int(..) | ty::TyKind::Uint(..) => {}
                                    _ => { unimplemented!("Only integers are currently supported as closure arguments."); }
                                }
                            });
                            match vars.result.1.kind() {
                                ty::TyKind::Int(..) | ty::TyKind::Uint(..) => {}
                                _ => { unimplemented!("Only integers are currently supported as closure return types."); }
                            }

                            let sf_pre_name = self.encoder.encode_spec_func_name(*def_id, SpecFunctionKind::Pre);
                            let qvars_pre: Vec<_> = vars.args
                                .iter()
                                .map(|(arg, arg_ty)| self.encode_forall_arg(*arg, arg_ty, &format!("{}_{}", vars.spec_id, vars.pre_id)))
                                .collect();
                            let pre_conjunct = vir::Expr::forall(
                                qvars_pre.clone(),
                                vec![], // TODO: encode triggers
                                vir::Expr::implies(
                                    encoded_pres.clone(),
                                    vir::Expr::FuncApp(
                                        sf_pre_name,
                                        qvars_pre.iter()
                                            .map(|x| vir::Expr::Local(x.clone(), vir::Position::default()))
                                            .collect(),
                                        (0 .. vars.args.len())
                                            .map(|i| vir::LocalVar::new(format!("_{}", i), vir::Type::Int))
                                            .collect(),
                                        vir::Type::Bool,
                                        vir::Position::default()
                                    )
                                )
                            );


                            let sf_post_name = self.encoder.encode_spec_func_name(*def_id, SpecFunctionKind::Post);

                            // The result is modeled as the final argument to the post() spec function
                            let result_var = mir::Local::from_usize(vars.args.len() + 2);

                            // The set of quantified variables
                            let qvars_post: Vec<_> = vars.args
                                .iter()
                                .map(|(arg, arg_ty)|
                                     self.encode_forall_arg(
                                         *arg, arg_ty,
                                         &format!("{}_{}", vars.spec_id, vars.post_id)))
                                .chain(std::iter::once(
                                    self.encode_forall_arg(
                                        result_var, tcx.mk_ty(ty::TyKind::Int(ty::IntTy::I32)),
                                        &format!("{}_{}", vars.spec_id, vars.post_id))))
                                .collect();

                            let post_conjunct = vir::Expr::forall(
                                qvars_post.clone(),
                                vec![], // TODO: encode triggers
                                vir::Expr::implies(
                                    // The quantified variables in the precondition have been encoded using
                                    // different IDs (vars.pre_id vs. vars.post_id), so we need to fix them
                                    (0 .. qvars_pre.len())
                                        .fold(encoded_pres, |e, i| {
                                            e.replace_place(&vir::Expr::Local(qvars_pre[i].clone(), vir::Position::default()),
                                                            &vir::Expr::Local(qvars_post[i].clone(), vir::Position::default()))
                                        }),
                                    vir::Expr::implies(
                                        vir::Expr::FuncApp(
                                            sf_post_name,
                                            qvars_post.iter()
                                                .map(|x| vir::Expr::Local(x.clone(), vir::Position::default()))
                                                .collect(),
                                            (0 ..= vars.args.len())
                                                .map(|i| vir::LocalVar::new(format!("_{}", i), vir::Type::Int))
                                                .collect(),
                                            vir::Type::Bool,
                                            vir::Position::default()
                                        ),
                                        posts.iter()
                                            .map(|x| self.encode_assertion(x))
                                            .collect::<Result<Vec<vir::Expr>, _>>()?
                                            .into_iter()
                                            .conjoin()
                                    )
                                )
                            );

                            vec![pre_conjunct, post_conjunct]
                                .into_iter()
                                .conjoin()
                        }
                        _ => unreachable!()
                    }
                } else {
                    // TODO
                    true.into()
                }
            }
        })
    }

    /// Translate an expression `expr` from a closure identified by `def_id` to its definition site.
    ///
    /// During the translation:
    /// * Usages of the closure's captured state will be translated to the captured place.
    /// * Closure arguments will be treated as quantified variables and will be translated using
    ///   the `self.encode_forall_arg(..)` method.
    ///
    /// The result is a tuple with:
    /// * the translated expression,
    /// * the def_id of the item in which the closure was defined,
    /// * the location at which the closure was defined.
    fn translate_expr_to_closure_def_site(
        &self,
        expr: vir::Expr,
        inner_def_id: DefId,
    ) -> SpannedEncodingResult<(vir::Expr, DefId, mir::Location)> {
        debug!("translate_expr_to_closure_def_site {} {:?}", expr, inner_def_id);
        let inner_mir = self.encoder.env().local_mir(inner_def_id.expect_local());
        let inner_mir_encoder = MirEncoder::new(self.encoder, &inner_mir, inner_def_id);
        let inner_attrs = self.encoder.env().tcx().get_attrs(inner_def_id);

        let opt_instantiation = self.encoder.get_single_closure_instantiation(
            inner_def_id
        );
        let (
            outer_def_id,
            outer_location,
            captured_operands,
            captured_operand_tys,
        ) = opt_instantiation.expect(
            &format!("cannot find definition site for closure {:?}", inner_def_id)
        );
        let outer_mir = self.encoder.env().local_mir(outer_def_id.expect_local());
        let outer_mir_encoder = MirEncoder::new(self.encoder, &outer_mir, outer_def_id);
        let outer_span = outer_mir_encoder.get_span_of_location(outer_location);
        trace!("Replacing variables of {:?} captured from {:?}", inner_def_id, outer_def_id);

        // Take the first argument, which is the closure's captured state.
        // The closure is a record containing all the captured variables.
        let closure_local = inner_mir.args_iter().next().unwrap();
        let closure_var = inner_mir_encoder.encode_local(closure_local)?;
        let closure_ty = &inner_mir.local_decls[closure_local].ty;
        let should_closure_be_dereferenced = inner_mir_encoder.can_be_dereferenced(closure_ty);
        let (deref_closure_var, _deref_closure_ty) = if should_closure_be_dereferenced {
            let res = inner_mir_encoder
                .encode_deref(closure_var.clone().into(), closure_ty)
                .with_span(outer_span)?;
            (res.0, res.1)
        } else {
            (closure_var.clone().into(), *closure_ty)
        };
        trace!("closure_ty: {:?}", closure_ty);
        trace!("deref_closure_var: {:?}", deref_closure_var);

        let captured_tys = captured_operand_tys;
        trace!("captured_tys: {:?}", captured_tys);
        assert_eq!(captured_tys.len(), captured_operands.len());

        // Replacements to translate from the closure to the definition site
        let mut replacements: Vec<(vir::Expr, vir::Expr)> = vec![];

        // Replacement 1: translate a local variable from the closure to a place in the outer MIR
        let inner_captured_places: Vec<vir::Expr> = captured_tys
            .iter()
            .enumerate()
            .map(|(index, &captured_ty)| {
                let field_name = format!("closure_{}", index);
                self.encoder.encode_raw_ref_field(field_name, captured_ty)
                    .with_span(outer_span)
                    .map(|encoded_field|
                        deref_closure_var.clone().field(encoded_field)
                    )
            })
            .collect::<Result<_, _>>()?;
        let outer_captured_places: Vec<_> = captured_operands
            .iter()
            .map(|operand| outer_mir_encoder.encode_operand_place(operand))
            .collect::<Result<Vec<_>, _>>()
            .with_span(outer_span)?
            .into_iter()
            .map(|x| x.unwrap())
            .collect();
        for (index, (inner_place, outer_place)) in inner_captured_places
            .iter()
            .zip(outer_captured_places.iter())
            .enumerate()
        {
            debug!(
                "Field {} of closure, encoded as {}: {}, corresponds to {}: {} \
                in the middle of the enclosing procedure",
                index,
                inner_place,
                inner_place.get_type(),
                outer_place,
                outer_place.get_type()
            );
            assert_eq!(inner_place.get_type(), outer_place.get_type());
        }
        replacements.extend(
            inner_captured_places
                .into_iter()
                .zip(outer_captured_places.into_iter())
        );

        // Replacement 2: rename the variables introduced by a quantification
        let opt_forall_id = read_prusti_attr("expr_id", inner_attrs);
        if let Some(forall_id) = opt_forall_id {
            // Skip the first argument, which is the captured state
            for local_arg_index in inner_mir.args_iter().skip(1) {
                let local_arg = &inner_mir.local_decls[local_arg_index];
                assert!(!local_arg.internal);
                let quantified_var = self.encode_forall_arg(
                    local_arg_index,
                    local_arg.ty,
                    &forall_id
                );
                let encoded_arg = inner_mir_encoder.encode_local(local_arg_index)?;
                let encoded_arg_value = match local_arg.ty.kind() {
                    ty::TyKind::Uint(_) |
                    ty::TyKind::Int(_) |
                    ty::TyKind::Bool |
                    ty::TyKind::Char => {
                        let span = inner_mir_encoder.get_local_span(local_arg_index);
                        let value_field = self.encoder.encode_value_field(local_arg.ty).with_span(span)?;
                        vir::Expr::local(encoded_arg).field(value_field)
                    }
                    _ => {
                        encoded_arg.into()
                    }
                };
                trace!(
                    "Place {}: {} will be renamed to {} because a quantifier introduced it",
                    encoded_arg_value,
                    encoded_arg_value.get_type(),
                    quantified_var
                );
                replacements.push((encoded_arg_value, quantified_var.into()));
            }
        }

        // Do the replacements
        let outer_expr = expr.replace_multiple_places(&replacements);
        debug!(
            "Expr at the definition site ({:?} {:?}): {}",
            outer_def_id,
            outer_location,
            outer_expr
        );
        Ok((outer_expr, outer_def_id, outer_location))
    }

    /// Given an expression and a program point, return the equivalent expression at a
    /// precedent program point.
    fn translate_expr_to_state(
        &self,
        expr: vir::Expr,
        def_id: DefId,
        expr_location: mir::Location,
        target_location: mir::BasicBlock,
    ) -> SpannedEncodingResult<vir::Expr> {
        debug!("translate_expr_to_state {} {:?} {:?}", expr, def_id, expr_location);
        let mir = self.encoder.env().local_mir(def_id.expect_local());

        // Translate an intermediate state to the state at the beginning of the method
        let state = MultiExprBackwardInterpreterState::new_single(
            expr.clone()
        );
        let interpreter = StraightLineBackwardInterpreter::new(
            self.encoder,
            &mir,
            def_id,
            self.parent_def_id,
        );
        let initial_state = run_backward_interpretation_point_to_point(
            &mir,
            &interpreter,
            target_location,
            expr_location.block,
            expr_location.statement_index,
            state,
            MultiExprBackwardInterpreterState::new(vec![]),
        )?.expect(
            &format!("cannot encode {:?} because its CFG contains a loop", def_id)
        );
        let pre_state_expr = initial_state.into_expressions().remove(0);

        trace!("Expr at the beginning: {}", pre_state_expr);
        Ok(pre_state_expr)
    }

    /// Encode the assertion of a contract or loop invariant.
    fn encode_expression(&self, assertion_expr: &typed::Expression)
        -> SpannedEncodingResult<vir::Expr>
    {
        debug!("encode_expression {:?}", assertion_expr);

        let mut curr_def_id = assertion_expr.expr.to_def_id();
        let mut curr_expr = self.encoder.encode_pure_expression(curr_def_id, self.parent_def_id)?;

        loop {
            let done = self.encoder.get_single_closure_instantiation(curr_def_id).is_none();
            if done {
                debug!("end of encode_expression loop: {:?} has no instantiation", curr_def_id);
                break;
            }
            let (
                outer_expr,
                outer_def_id,
                outer_location,
            ) = self.translate_expr_to_closure_def_site(curr_expr, curr_def_id)?;
            let done = self.encoder.get_single_closure_instantiation(outer_def_id).is_none();
            curr_expr = self.translate_expr_to_state(
                outer_expr,
                outer_def_id,
                outer_location,
                if done {
                    self.assertion_location.unwrap_or(mir::START_BLOCK)
                } else {
                    mir::START_BLOCK
                },
            )?;
            curr_def_id = outer_def_id;
        }

        // FIXME: "self" is skipped for closures, see TypeEncoder
        let skip_first = self.encoder.encode_item_name(curr_def_id).contains("_closure_");

        // At this point `curr_def_id` should be either a SPEC item (when encoding a contract) or
        // the method being verified (when encoding a loop invariant).
        let mir = self.encoder.env().local_mir(curr_def_id.expect_local());
        let mir_encoder = MirEncoder::new(self.encoder, &mir, curr_def_id);

        // Replacements to use the provided `target_args` and `target_return`
        let mut replacements: Vec<(vir::Expr, vir::Expr)> = vec![];

        // Replacement 1: replace the arguments with the `target_args`.
        replacements.extend(
            mir.args_iter()
                .zip(self.target_args
                         .iter()
                         .skip(if skip_first { 1 } else { 0 }))
                .take(if let Some(_) = self.target_return { mir.arg_count - 1 } else { mir.arg_count })
                .map(|(local, target_arg)| {
                    let local_ty = mir.local_decls[local].ty;
                    // will panic if attempting to encode unsupported type
                    let spec_local = mir_encoder.encode_local(local).unwrap();
                    let spec_local_place: vir::Expr = if self.targets_are_values {
                        self.encoder.encode_value_expr(
                            vir::Expr::local(spec_local),
                            local_ty
                        ).with_span(mir_encoder.get_local_span(local))?
                    } else {
                        spec_local.into()
                    };
                    Ok((spec_local_place, target_arg.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
        );

        // Replacement 2: replace the fake return variable (last argument) of SPEC items with
        // `target_return`
        if let Some(target_return) = self.target_return {
            let fake_return_local = mir.args_iter().last().unwrap();
            let fake_return_ty = mir.local_decls[fake_return_local].ty;
            // will panic if attempting to encode unsupported type
            let spec_fake_return = mir_encoder.encode_local(fake_return_local).unwrap();
            let spec_fake_return_place: vir::Expr = if self.targets_are_values {
                self.encoder.encode_value_expr(
                    vir::Expr::local(spec_fake_return),
                    fake_return_ty
                ).with_span(mir_encoder.get_local_span(fake_return_local))?
            } else {
                spec_fake_return.clone().into()
            };
            replacements.push((spec_fake_return_place, target_return.clone()));
        }

        // Do the replacements
        curr_expr = curr_expr.replace_multiple_places(&replacements);

        // use the provided `self.pre_label` to encode old expressions
        curr_expr = curr_expr.map_old_expr_label(|label| {
            if label == PRECONDITION_LABEL {
                self.pre_label.to_string()
            } else {
                label.clone()
            }
        });
        println!("spec_encoder : {:?}", self.parent_def_id);
        debug!("MIR expr {:?} --> {}", assertion_expr.id, curr_expr);
        Ok(curr_expr.set_default_pos(
            self.encoder
                .error_manager()
                .register(
                    self.encoder.env().tcx().def_span(assertion_expr.expr),
                    ErrorCtxt::GenericExpression,
                    self.parent_def_id),
        ))
    }
}

struct StraightLineBackwardInterpreter<'p, 'v: 'p, 'tcx: 'v> {
    interpreter: PureFunctionBackwardInterpreter<'p, 'v, 'tcx>,
}

/// XXX: This encoding works backward, but there is the risk of generating expressions whose length
/// is exponential in the number of branches. If this becomes a problem, consider doing a forward
/// encoding (keeping path conditions expressions).
impl<'p, 'v: 'p, 'tcx: 'v> StraightLineBackwardInterpreter<'p, 'v, 'tcx> {
    fn new(
        encoder: &'p Encoder<'v, 'tcx>,
        mir: &'p mir::Body<'tcx>,
        def_id: DefId,
        parent_def_id: DefId,
    ) -> Self {
        StraightLineBackwardInterpreter {
            interpreter: PureFunctionBackwardInterpreter::new(
                encoder, mir, def_id, false, parent_def_id
            ),
        }
    }
}

impl<'p, 'v: 'p, 'tcx: 'v> BackwardMirInterpreter<'tcx>
    for StraightLineBackwardInterpreter<'p, 'v, 'tcx>
{
    type State = MultiExprBackwardInterpreterState;
    type Error = SpannedEncodingError;

    fn apply_terminator(
        &self,
        bb: mir::BasicBlock,
        term: &mir::Terminator<'tcx>,
        states: HashMap<mir::BasicBlock, &Self::State>,
    ) -> Result<Self::State, Self::Error> {
        trace!("apply_terminator {:?}, states: {:?}", term, states);
        if !states.is_empty() && states.values().all(|state| !state.exprs().is_empty()) {
            // All states are initialized
            self.interpreter.apply_terminator(bb, term, states)
        } else {
            // One of the states is not yet initialized
            trace!("Skip terminator {:?}", term);
            Ok(MultiExprBackwardInterpreterState::new(vec![]))
        }
    }
    fn apply_statement(
        &self,
        bb: mir::BasicBlock,
        stmt_index: usize,
        stmt: &mir::Statement<'tcx>,
        state: &mut Self::State,
    ) -> Result<(), Self::Error> {
        trace!("apply_statement {:?}, state: {:?}", stmt, state);
        if !state.exprs().is_empty() {
            // The state is initialized
            self.interpreter
                .apply_statement(bb, stmt_index, stmt, state)?;
        } else {
            // The state is not yet initialized
            trace!("Skip statement {:?}", stmt);
        }
        Ok(())
    }
}
