// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use encoder::borrows::{compute_procedure_contract, ProcedureContract};
use encoder::builtin_encoder::BuiltinFunctionKind;
use encoder::error_manager::ErrorCtxt;
use encoder::error_manager::PanicCause;
use encoder::foldunfold;
use encoder::mir_encoder::MirEncoder;
use encoder::mir_encoder::{PRECONDITION_LABEL, WAND_LHS_LABEL};
use encoder::mir_interpreter::{
    run_backward_interpretation, BackwardMirInterpreter, MultiExprBackwardInterpreterState,
};
use encoder::vir;
use encoder::vir::ExprIterator;
use encoder::Encoder;
use prusti_interface::config;
use prusti_interface::specifications::SpecificationSet;
use rustc::hir;
use rustc::hir::def_id::DefId;
use rustc::mir;
use rustc::ty;
use std::collections::HashMap;

pub struct PureFunctionEncoder<'p, 'v: 'p, 'r: 'v, 'a: 'r, 'tcx: 'a> {
    encoder: &'p Encoder<'v, 'r, 'a, 'tcx>,
    proc_def_id: DefId,
    mir: &'p mir::Mir<'tcx>,
    interpreter: PureFunctionBackwardInterpreter<'p, 'v, 'r, 'a, 'tcx>,
}

impl<'p, 'v: 'p, 'r: 'v, 'a: 'r, 'tcx: 'a> PureFunctionEncoder<'p, 'v, 'r, 'a, 'tcx> {
    pub fn new(
        encoder: &'p Encoder<'v, 'r, 'a, 'tcx>,
        proc_def_id: DefId,
        mir: &'p mir::Mir<'tcx>,
        is_encoding_assertion: bool,
    ) -> Self {
        trace!("PureFunctionEncoder constructor: {:?}", proc_def_id);
        let interpreter = PureFunctionBackwardInterpreter::new(
            encoder,
            mir,
            proc_def_id,
            "_pure".to_string(),
            is_encoding_assertion,
        );
        PureFunctionEncoder {
            encoder,
            proc_def_id,
            mir,
            interpreter,
        }
    }

    /// Used to encode expressions in assertions
    pub fn encode_body(&self) -> vir::Expr {
        let function_name = self.encoder.env().get_absolute_item_name(self.proc_def_id);
        debug!("Encode body of pure function {}", function_name);

        let state = run_backward_interpretation(self.mir, &self.interpreter)
            .expect(&format!("Procedure {:?} contains a loop", self.proc_def_id));
        let body_expr = state.into_expressions().remove(0);
        debug!(
            "Pure function {} has been encoded with expr: {}",
            function_name, body_expr
        );
        let subst_strings = self.encoder.type_substitution_strings();
        let patched_body_expr = body_expr.patch_types(&subst_strings);
        patched_body_expr
    }

    pub fn encode_function(&self) -> vir::Function {
        let function_name = self.encode_function_name();
        debug!("Encode pure function {}", function_name);

        let mut state = run_backward_interpretation(self.mir, &self.interpreter)
            .expect(&format!("Procedure {:?} contains a loop", self.proc_def_id));

        // Fix arguments
        for arg in self.mir.args_iter() {
            let arg_ty = self.interpreter.mir_encoder().get_local_ty(arg);
            let value_field = self.encoder.encode_value_field(arg_ty);
            let target_place: vir::Expr =
                vir::Expr::local(self.interpreter.mir_encoder().encode_local(arg))
                    .field(value_field);
            let new_place: vir::Expr = self.encode_local(arg).into();
            state.substitute_place(&target_place, new_place);
        }

        let body_expr = state.into_expressions().remove(0);
        debug!(
            "Pure function {} has been encoded with expr: {}",
            function_name, body_expr
        );

        self.encode_function_given_body(Some(body_expr))
    }

    pub fn encode_bodyless_function(&self) -> vir::Function {
        let function_name = self.encode_function_name();
        debug!("Encode trusted (bodyless) pure function {}", function_name);

        self.encode_function_given_body(None)
    }

    // Private

    fn encode_function_given_body(&self, body: Option<vir::Expr>) -> vir::Function {
        let function_name = self.encode_function_name();
        let is_bodyless = body.is_none();
        if is_bodyless {
            debug!("Encode pure function {} given body None", function_name);
        } else {
            debug!(
                "Encode pure function {} given body Some({})",
                function_name,
                body.as_ref().unwrap()
            );
        }

        // TODO: Clean up code duplication:
        //let contract = self.encoder.get_procedure_contract_for_def(self.proc_def_id);
        let contract = {
            let opt_fun_spec = self.encoder.get_spec_by_def_id(self.proc_def_id);
            let fun_spec = match opt_fun_spec {
                Some(fun_spec) => fun_spec.clone(),
                None => {
                    debug!("Procedure {:?} has no specification", self.proc_def_id);
                    SpecificationSet::Procedure(vec![], vec![])
                }
            };
            let tymap = self.encoder.current_tymap();
            let contract = compute_procedure_contract(
                self.proc_def_id,
                self.encoder.env().tcx(),
                fun_spec,
                Some(&tymap),
            );
            contract.to_def_site_contract()
        };
        let subst_strings = self.encoder.type_substitution_strings();

        let (type_precondition, func_precondition) = self.encode_precondition_expr(&contract);
        let patched_type_precondition = type_precondition.patch_types(&subst_strings);
        let mut precondition = vec![patched_type_precondition, func_precondition];
        let mut postcondition = vec![self.encode_postcondition_expr(&contract)];

        let formal_args: Vec<_> = self
            .mir
            .args_iter()
            .map(|local| {
                let var_name = self.interpreter.mir_encoder().encode_local_var_name(local);
                let mir_type = self.interpreter.mir_encoder().get_local_ty(local);
                let var_type = self
                    .encoder
                    .encode_value_type(self.encoder.resolve_typaram(mir_type));
                let var_type = var_type.patch(&subst_strings);
                vir::LocalVar::new(var_name, var_type)
            })
            .collect();
        let return_type = self.encode_function_return_type();

        let res_value_range_pos = self.encoder.error_manager().register(
            self.mir.span,
            ErrorCtxt::PureFunctionPostconditionValueRangeOfResult,
        );
        let pure_fn_return_variable =
            vir::LocalVar::new("__result", self.encode_function_return_type());
        // Add value range of the arguments and return value to the pre/postconditions
        if config::check_binary_operations() {
            let return_bounds: Vec<_> = self
                .encoder
                .encode_type_bounds(
                    &vir::Expr::local(pure_fn_return_variable),
                    self.mir.return_ty(),
                )
                .into_iter()
                .map(|p| p.set_default_pos(res_value_range_pos.clone()))
                .collect();
            postcondition.extend(return_bounds);

            for (formal_arg, local) in formal_args.iter().zip(self.mir.args_iter()) {
                let typ = self.interpreter.mir_encoder().get_local_ty(local);
                let bounds = self
                    .encoder
                    .encode_type_bounds(&vir::Expr::local(formal_arg.clone()), &typ);
                precondition.extend(bounds);
            }
        } else if config::encode_unsigned_num_constraint() {
            if let ty::TypeVariants::TyUint(_) = self.mir.return_ty().sty {
                let expr = vir::Expr::le_cmp(0.into(), pure_fn_return_variable.into());
                postcondition.push(expr.set_default_pos(res_value_range_pos));
            }
            for (formal_arg, local) in formal_args.iter().zip(self.mir.args_iter()) {
                let typ = self.interpreter.mir_encoder().get_local_ty(local);
                if let ty::TypeVariants::TyUint(_) = typ.sty {
                    precondition.push(vir::Expr::le_cmp(0.into(), formal_arg.into()));
                }
            }
        }

        debug_assert!(
            !postcondition.iter().any(|p| p.pos().is_default()),
            "Some postcondition has no position: {:?}",
            postcondition
        );

        let mut function = vir::Function {
            name: function_name.clone(),
            formal_args,
            return_type,
            pres: precondition,
            posts: postcondition,
            body,
        };

        self.encoder
            .log_vir_program_before_foldunfold(function.to_string());

        if config::simplify_encoding() {
            function = vir::optimisations::functions::Simplifier::simplify(function);
        }

        // Add folding/unfolding
        foldunfold::add_folding_unfolding_to_function(
            function,
            self.encoder.get_used_viper_predicates_map(),
        )
    }

    /// Encode the precondition with two expressions:
    /// - one for the type encoding
    /// - one for the functional specification.
    fn encode_precondition_expr(
        &self,
        contract: &ProcedureContract<'tcx>,
    ) -> (vir::Expr, vir::Expr) {
        let type_spec = contract.args.iter().flat_map(|&local| {
            let local_ty = self.interpreter.mir_encoder().get_local_ty(local.into());
            let fraction = if let ty::TypeVariants::TyRef(_, _, hir::Mutability::MutImmutable) =
                local_ty.sty
            {
                vir::PermAmount::Read
            } else {
                vir::PermAmount::Write
            };
            self.interpreter
                .mir_encoder()
                .encode_place_predicate_permission(self.encode_local(local.into()).into(), fraction)
        });
        let mut func_spec: Vec<vir::Expr> = vec![];

        // Encode functional specification
        let encoded_args: Vec<vir::Expr> = contract
            .args
            .iter()
            .map(|local| self.encode_local(local.clone().into()).into())
            .collect();
        for item in contract.functional_precondition() {
            debug!("Encode spec item: {:?}", item);
            func_spec.push(self.encoder.encode_assertion(
                &item.assertion,
                &self.mir,
                &"",
                &encoded_args,
                None,
                true,
                None,
                ErrorCtxt::GenericExpression,
            ));
        }

        (
            type_spec.into_iter().conjoin(),
            func_spec.into_iter().conjoin(),
        )
    }

    /// Encode the postcondition with one expression just for the functional specification (no
    /// type encoding).
    fn encode_postcondition_expr(&self, contract: &ProcedureContract<'tcx>) -> vir::Expr {
        let mut func_spec: Vec<vir::Expr> = vec![];

        // Encode functional specification
        let encoded_args: Vec<vir::Expr> = contract
            .args
            .iter()
            .map(|local| self.encode_local(local.clone().into()).into())
            .collect();
        let encoded_return = self.encode_local(contract.returned_value.clone().into());
        debug!("encoded_return: {:?}", encoded_return);
        for item in contract.functional_postcondition() {
            let encoded_postcond = self.encoder.encode_assertion(
                &item.assertion,
                &self.mir,
                &"",
                &encoded_args,
                Some(&encoded_return.clone().into()),
                true,
                None,
                ErrorCtxt::GenericExpression,
            );
            debug_assert!(!encoded_postcond.pos().is_default());
            func_spec.push(encoded_postcond);
        }

        let post = func_spec.into_iter().conjoin();

        // TODO: use a better span
        let postcondition_pos = self
            .encoder
            .error_manager()
            .register(self.mir.span, ErrorCtxt::GenericExpression);

        // Fix return variable
        let pure_fn_return_variable =
            vir::LocalVar::new("__result", self.encode_function_return_type());
        post.replace_place(&encoded_return.into(), &pure_fn_return_variable.into())
            .set_default_pos(postcondition_pos)
    }

    fn encode_local(&self, local: mir::Local) -> vir::LocalVar {
        let var_name = self.interpreter.mir_encoder().encode_local_var_name(local);
        let var_type = self
            .encoder
            .encode_value_type(self.interpreter.mir_encoder().get_local_ty(local));
        vir::LocalVar::new(var_name, var_type)
    }

    pub fn encode_function_name(&self) -> String {
        self.encoder.encode_item_name(self.proc_def_id)
    }

    pub fn encode_function_return_type(&self) -> vir::Type {
        let ty = self.encoder.resolve_typaram(self.mir.return_ty());
        self.encoder.encode_value_type(ty)
    }
}

pub(super) struct PureFunctionBackwardInterpreter<'p, 'v: 'p, 'r: 'v, 'a: 'r, 'tcx: 'a> {
    encoder: &'p Encoder<'v, 'r, 'a, 'tcx>,
    mir: &'p mir::Mir<'tcx>,
    mir_encoder: MirEncoder<'p, 'v, 'r, 'a, 'tcx>,
    namespace: String,
    /// True if the encoder is currently encoding an assertion and not a pure function body. This
    /// flag is used to distinguish when assert terminators should be translated into `false` and
    /// when to a undefined function calls. This distinction allows overflow checks to be checked
    /// on the caller side and assumed on the definition side.
    is_encoding_assertion: bool,
}

/// XXX: This encoding works backward, but there is the risk of generating expressions whose length
/// is exponential in the number of branches. If this becomes a problem, consider doing a forward
/// encoding (keeping path conditions expressions).
impl<'p, 'v: 'p, 'r: 'v, 'a: 'r, 'tcx: 'a> PureFunctionBackwardInterpreter<'p, 'v, 'r, 'a, 'tcx> {
    pub(super) fn new(
        encoder: &'p Encoder<'v, 'r, 'a, 'tcx>,
        mir: &'p mir::Mir<'tcx>,
        def_id: DefId,
        namespace: String,
        is_encoding_assertion: bool,
    ) -> Self {
        PureFunctionBackwardInterpreter {
            encoder,
            mir,
            mir_encoder: MirEncoder::new_with_namespace(encoder, mir, def_id, namespace.clone()),
            namespace,
            is_encoding_assertion,
        }
    }

    pub(super) fn mir_encoder(&self) -> &MirEncoder<'p, 'v, 'r, 'a, 'tcx> {
        &self.mir_encoder
    }

}

impl<'p, 'v: 'p, 'r: 'v, 'a: 'r, 'tcx: 'a> BackwardMirInterpreter<'tcx>
    for PureFunctionBackwardInterpreter<'p, 'v, 'r, 'a, 'tcx>
{
    type State = MultiExprBackwardInterpreterState;

    fn apply_terminator(
        &self,
        _bb: mir::BasicBlock,
        term: &mir::Terminator<'tcx>,
        states: HashMap<mir::BasicBlock, &Self::State>,
    ) -> Self::State {
        trace!("apply_terminator {:?}, states: {:?}", term, states);
        use rustc::mir::TerminatorKind;

        // Generate a function call that leaves the expression undefined.
        let unreachable_expr = |pos| {
            let encoded_type = self.encoder.encode_value_type(self.mir.return_ty());
            let function_name =
                self.encoder
                    .encode_builtin_function_use(BuiltinFunctionKind::Unreachable(
                        encoded_type.clone(),
                    ));
            vir::Expr::func_app(function_name, vec![], vec![], encoded_type, pos)
        };

        // Generate a function call that leaves the expression undefined.
        let undef_expr = |pos| {
            let encoded_type = self.encoder.encode_value_type(self.mir.return_ty());
            let function_name = self
                .encoder
                .encode_builtin_function_use(BuiltinFunctionKind::Undefined(encoded_type.clone()));
            vir::Expr::func_app(function_name, vec![], vec![], encoded_type, pos)
        };

        match term.kind {
            TerminatorKind::Unreachable => {
                assert!(states.is_empty());
                let pos = self
                    .encoder
                    .error_manager()
                    .register(term.source_info.span, ErrorCtxt::Unexpected);
                MultiExprBackwardInterpreterState::new_single(undef_expr(pos))
            }

            TerminatorKind::Abort | TerminatorKind::Resume { .. } => {
                assert!(states.is_empty());
                let pos = self
                    .encoder
                    .error_manager()
                    .register(term.source_info.span, ErrorCtxt::Unexpected);
                MultiExprBackwardInterpreterState::new_single(unreachable_expr(pos))
            }

            TerminatorKind::Drop { ref target, .. } => {
                assert!(1 <= states.len() && states.len() <= 2);
                states[target].clone()
            }

            TerminatorKind::Goto { ref target } => {
                assert_eq!(states.len(), 1);
                states[target].clone()
            }

            TerminatorKind::FalseEdges {
                ref real_target, ..
            } => {
                assert_eq!(states.len(), 2);
                states[real_target].clone()
            }

            TerminatorKind::FalseUnwind {
                ref real_target, ..
            } => {
                assert_eq!(states.len(), 1);
                states[real_target].clone()
            }

            TerminatorKind::Return => {
                assert!(states.is_empty());
                trace!("Return type: {:?}", self.mir.return_ty());
                let return_type = self.encoder.encode_type(self.mir.return_ty());
                let return_var = vir::LocalVar::new(format!("{}_0", self.namespace), return_type);
                let field = self.encoder.encode_value_field(self.mir.return_ty());
                MultiExprBackwardInterpreterState::new_single(
                    vir::Expr::local(return_var.into()).field(field).into(),
                )
            }

            TerminatorKind::SwitchInt {
                ref targets,
                ref discr,
                ref values,
                switch_ty,
            } => {
                trace!(
                    "SwitchInt ty '{:?}', discr '{:?}', values '{:?}'",
                    switch_ty,
                    discr,
                    values
                );
                let mut cfg_targets: Vec<(vir::Expr, mir::BasicBlock)> = vec![];
                let discr_val = self.mir_encoder.encode_operand_expr(discr);
                for (i, &value) in values.iter().enumerate() {
                    let target = targets[i as usize];
                    // Convert int to bool, if required
                    let viper_guard = match switch_ty.sty {
                        ty::TypeVariants::TyBool => {
                            if value == 0 {
                                // If discr is 0 (false)
                                vir::Expr::not(discr_val.clone().into())
                            } else {
                                // If discr is not 0 (true)
                                discr_val.clone().into()
                            }
                        }

                        ty::TypeVariants::TyInt(_) | ty::TypeVariants::TyUint(_) => {
                            vir::Expr::eq_cmp(
                                discr_val.clone().into(),
                                self.encoder.encode_int_cast(value, switch_ty),
                            )
                        }

                        ref x => unreachable!("{:?}", x),
                    };
                    cfg_targets.push((viper_guard, target))
                }
                let default_target = targets[values.len()];

                let default_target_terminator = self.mir.basic_blocks()[default_target]
                    .terminator
                    .as_ref()
                    .unwrap();
                trace!("default_target_terminator: {:?}", default_target_terminator);
                let default_is_unreachable = match default_target_terminator.kind {
                    TerminatorKind::Unreachable => true,
                    _ => false,
                };

                trace!("cfg_targets: {:?}", cfg_targets);

                let refined_default_target = if default_is_unreachable && !cfg_targets.is_empty() {
                    // Here we can assume that the `cfg_targets` are exhausive, and that
                    // `default_target` is unreachable
                    trace!("The default target is unreachable");
                    cfg_targets.pop().unwrap().1
                } else {
                    default_target
                };

                trace!("cfg_targets: {:?}", cfg_targets);

                MultiExprBackwardInterpreterState::new(
                    (0..states[&refined_default_target].exprs().len())
                        .map(|expr_index| {
                            cfg_targets.iter().fold(
                                states[&refined_default_target].exprs()[expr_index].clone(),
                                |else_expr, (guard, target)| {
                                    let then_expr = states[&target].exprs()[expr_index].clone();
                                    if then_expr == else_expr {
                                        // Optimization
                                        else_expr
                                    } else {
                                        vir::Expr::ite(guard.clone(), then_expr, else_expr)
                                    }
                                },
                            )
                        })
                        .collect(),
                )
            }

            TerminatorKind::DropAndReplace { ..  } => {
                unimplemented!()
            },

            TerminatorKind::Call {
                ref args,
                ref destination,
                func:
                    mir::Operand::Constant(box mir::Constant {
                        literal:
                            mir::Literal::Value {
                                value:
                                    ty::Const {
                                        ty:
                                            &ty::TyS {
                                                sty: ty::TyFnDef(def_id, substs),
                                                ..
                                            },
                                        ..
                                    },
                            },
                        ..
                    }),
                ..
            } => {
                let func_proc_name: &str = &self.encoder.env().tcx().absolute_item_path_str(def_id);

                let own_substs =
                    ty::subst::Substs::identity_for_item(self.encoder.env().tcx(), def_id);

                {
                    // FIXME; hideous monstrosity...
                    let mut tymap_stack = self.encoder.typaram_repl.borrow_mut();
                    let mut tymap = HashMap::new();

                    for (kind1, kind2) in own_substs.iter().zip(substs) {
                        if let (
                            ty::subst::UnpackedKind::Type(ty1),
                            ty::subst::UnpackedKind::Type(ty2),
                        ) = (kind1.unpack(), kind2.unpack())
                        {
                            tymap.insert(ty1, ty2);
                        }
                    }
                    tymap_stack.push(tymap);
                }

                let state = if destination.is_some() {
                    let (ref lhs_place, target_block) = destination.as_ref().unwrap();
                    let (encoded_lhs, ty, _) = self.mir_encoder.encode_place(lhs_place);
                    let lhs_value = encoded_lhs
                        .clone()
                        .field(self.encoder.encode_value_field(ty));
                    let encoded_args: Vec<vir::Expr> = args
                        .iter()
                        .map(|arg| self.mir_encoder.encode_operand_expr(arg))
                        .collect();

                    match func_proc_name {
                        "prusti_contracts::internal::old" => {
                            trace!("Encoding old expression {:?}", args[0]);
                            assert_eq!(args.len(), 1);
                            let encoded_rhs = self
                                .mir_encoder
                                .encode_old_expr(encoded_args[0].clone(), PRECONDITION_LABEL);
                            let mut state = states[&target_block].clone();
                            state.substitute_value(&lhs_value, encoded_rhs);
                            state
                        }

                        "prusti_contracts::internal::before_expiry" => {
                            trace!("Encoding before_expiry expression {:?}", args[0]);
                            assert_eq!(args.len(), 1);
                            let encoded_rhs = self
                                .mir_encoder
                                .encode_old_expr(encoded_args[0].clone(), WAND_LHS_LABEL);
                            let mut state = states[&target_block].clone();
                            state.substitute_value(&lhs_value, encoded_rhs);
                            state
                        }

                        // generic function call
                        _ => {
                            let function_name = self.encoder.encode_pure_function_use(def_id);
                            trace!("Encoding pure function call '{}'", function_name);

                            let return_type = self.encoder.encode_pure_function_return_type(def_id);
                            let formal_args: Vec<vir::LocalVar> = args
                                .iter()
                                .enumerate()
                                .map(|(i, arg)| {
                                    vir::LocalVar::new(
                                        format!("x{}", i),
                                        self.mir_encoder.encode_operand_expr_type(arg),
                                    )
                                })
                                .collect();

                            let pos = self
                                .encoder
                                .error_manager()
                                .register(term.source_info.span, ErrorCtxt::PureFunctionCall);
                            let encoded_rhs = vir::Expr::func_app(
                                function_name,
                                encoded_args,
                                formal_args,
                                return_type,
                                pos,
                            );

                            let mut state = states[&target_block].clone();
                            state.substitute_value(&lhs_value, encoded_rhs);
                            state
                        }
                    }
                } else {
                    // Encoding of a non-terminating function call
                    let error_ctxt = match func_proc_name {
                        "std::rt::begin_panic" | "std::panicking::begin_panic" => {
                            // This is called when a Rust assertion fails
                            // args[0]: message
                            // args[1]: position of failing assertions

                            // Pattern match on the macro that generated the panic
                            // TODO: use a better approach to match macros
                            let macro_backtrace = term.source_info.span.macro_backtrace();
                            debug!("macro_backtrace: {:?}", macro_backtrace);

                            let panic_cause = if !macro_backtrace.is_empty() {
                                let macro_name = term.source_info.span.macro_backtrace()[0]
                                    .macro_decl_name
                                    .clone();
                                // HACK to match the filename of the span
                                let def_site_span = format!(
                                    "{:?}",
                                    term.source_info.span.macro_backtrace()[0].def_site_span
                                );

                                match macro_name.as_str() {
                                    "panic!" if def_site_span.contains("<panic macros>") => {
                                        if macro_backtrace.len() > 1 {
                                            let second_macro_name =
                                                term.source_info.span.macro_backtrace()[1]
                                                    .macro_decl_name
                                                    .clone();
                                            // HACK to match the filename of the span
                                            let second_def_site_span = format!(
                                                "{:?}",
                                                term.source_info.span.macro_backtrace()[1]
                                                    .def_site_span
                                            );

                                            match second_macro_name.as_str() {
                                                "panic!"
                                                    if second_def_site_span
                                                        .contains("<panic macros>") =>
                                                {
                                                    PanicCause::Panic
                                                }
                                                "assert!" if second_def_site_span == "None" => {
                                                    PanicCause::Assert
                                                }
                                                "unreachable!"
                                                    if second_def_site_span
                                                        .contains("<unreachable macros>") =>
                                                {
                                                    PanicCause::Unreachable
                                                }
                                                "unimplemented!"
                                                    if second_def_site_span
                                                        .contains("<unimplemented macros>") =>
                                                {
                                                    PanicCause::Unimplemented
                                                }
                                                _ => PanicCause::Panic,
                                            }
                                        } else {
                                            PanicCause::Panic
                                        }
                                    }
                                    _ => PanicCause::Unknown,
                                }
                            } else {
                                // Something else called panic!()
                                PanicCause::Unknown
                            };
                            ErrorCtxt::PanicInPureFunction(panic_cause)
                        }

                        _ => ErrorCtxt::DivergingCallInPureFunction,
                    };
                    let pos = self
                        .encoder
                        .error_manager()
                        .register(term.source_info.span, error_ctxt);
                    MultiExprBackwardInterpreterState::new_single(unreachable_expr(pos))
                };

                // FIXME; hideous monstrosity...
                {
                    let mut tymap_stack = self.encoder.typaram_repl.borrow_mut();
                    tymap_stack.pop();
                }
                state
            }

            TerminatorKind::Call { .. } => {
                // Other kind of calls?
                unimplemented!();
            }

            TerminatorKind::Assert {
                ref cond,
                expected,
                ref target,
                ref msg,
                ..
            } => {
                let cond_val = self.mir_encoder.encode_operand_expr(cond);
                let viper_guard = if expected {
                    cond_val
                } else {
                    vir::Expr::not(cond_val)
                };

                let pos = self.encoder.error_manager().register(
                    term.source_info.span,
                    ErrorCtxt::PureFunctionAssertTerminator(msg.description().to_string()),
                );

                MultiExprBackwardInterpreterState::new(
                    states[target]
                        .exprs()
                        .iter()
                        .map(|expr| {
                            let failure_result = if self.is_encoding_assertion {
                                // We are encoding an assertion, so all failures should be
                                // equivalent to false.
                                false.into()
                            } else {
                                // We are encoding a pure function, so all failures should
                                // be unreachable.
                                unreachable_expr(pos.clone())
                            };
                            vir::Expr::ite(viper_guard.clone(), expr.clone(), failure_result)
                        })
                        .collect(),
                )
            }

            TerminatorKind::Yield { .. } | TerminatorKind::GeneratorDrop => {
                unimplemented!("{:?}", term.kind)
            }
        }
    }

    fn apply_statement(
        &self,
        _bb: mir::BasicBlock,
        _stmt_index: usize,
        stmt: &mir::Statement<'tcx>,
        state: &mut Self::State,
    ) {
        trace!("apply_statement {:?}, state: {}", stmt, state);

        match stmt.kind {
            mir::StatementKind::StorageLive(..)
            | mir::StatementKind::StorageDead(..)
            | mir::StatementKind::ReadForMatch(..)
            | mir::StatementKind::EndRegion(..) => {
                // Nothing to do
            }

            mir::StatementKind::Assign(ref lhs, ref rhs) => {
                let (encoded_lhs, ty, _) = self.mir_encoder.encode_place(lhs);

                if !state.use_place(&encoded_lhs) {
                    // If the lhs is not mentioned in our state, do nothing
                    trace!("The state does not mention {:?}", encoded_lhs);
                    return;
                }

                let opt_lhs_value_place = match ty.sty {
                    ty::TypeVariants::TyBool
                    | ty::TypeVariants::TyInt(..)
                    | ty::TypeVariants::TyUint(..)
                    | ty::TypeVariants::TyRawPtr(..)
                    | ty::TypeVariants::TyRef(..) => Some(
                        encoded_lhs
                            .clone()
                            .field(self.encoder.encode_value_field(ty)),
                    ),
                    _ => None,
                };

                match rhs {
                    &mir::Rvalue::Use(ref operand) => {
                        let opt_encoded_rhs = self.mir_encoder.encode_operand_place(operand);

                        match opt_encoded_rhs {
                            Some(encoded_rhs) => {
                                // Substitute a place
                                state.substitute_place(&encoded_lhs, encoded_rhs);
                            }
                            None => {
                                // Substitute a place of a value with an expression
                                let rhs_expr = self.mir_encoder.encode_operand_expr(operand);
                                state.substitute_value(&opt_lhs_value_place.unwrap(), rhs_expr);
                            }
                        }
                    }

                    &mir::Rvalue::Aggregate(ref aggregate, ref operands) => {
                        debug!("Encode aggregate {:?}, {:?}", aggregate, operands);
                        match aggregate.as_ref() {
                            &mir::AggregateKind::Tuple => {
                                let field_types = if let ty::TypeVariants::TyTuple(ref x) = ty.sty {
                                    x
                                } else {
                                    unreachable!()
                                };
                                for (field_num, operand) in operands.iter().enumerate() {
                                    let field_name = format!("tuple_{}", field_num);
                                    let field_ty = field_types[field_num];
                                    let encoded_field =
                                        self.encoder.encode_raw_ref_field(field_name, field_ty);
                                    let field_place = encoded_lhs.clone().field(encoded_field);

                                    match self.mir_encoder.encode_operand_place(operand) {
                                        Some(encoded_rhs) => {
                                            // Substitute a place
                                            state.substitute_place(&field_place, encoded_rhs);
                                        }
                                        None => {
                                            // Substitute a place of a value with an expression
                                            let rhs_expr =
                                                self.mir_encoder.encode_operand_expr(operand);
                                            let value_field =
                                                self.encoder.encode_value_field(field_ty);
                                            state.substitute_value(
                                                &field_place.field(value_field),
                                                rhs_expr,
                                            );
                                        }
                                    }
                                }
                            }

                            &mir::AggregateKind::Adt(adt_def, variant_index, subst, _) => {
                                let num_variants = adt_def.variants.len();
                                let variant_def = &adt_def.variants[variant_index];
                                let mut encoded_lhs_variant = encoded_lhs.clone();
                                if num_variants != 1 {
                                    let discr_field = self.encoder.encode_discriminant_field();
                                    state.substitute_value(
                                        &encoded_lhs.clone().field(discr_field),
                                        variant_index.into(),
                                    );
                                    encoded_lhs_variant =
                                        encoded_lhs_variant.variant(&variant_def.name.as_str());
                                }
                                for (field_index, field) in variant_def.fields.iter().enumerate() {
                                    let operand = &operands[field_index];
                                    let field_name = &field.ident.as_str();
                                    let tcx = self.encoder.env().tcx();
                                    let field_ty = field.ty(tcx, subst);
                                    let encoded_field =
                                        self.encoder.encode_struct_field(field_name, field_ty);

                                    let field_place =
                                        encoded_lhs_variant.clone().field(encoded_field);
                                    match self.mir_encoder.encode_operand_place(operand) {
                                        Some(encoded_rhs) => {
                                            // Substitute a place
                                            state.substitute_place(&field_place, encoded_rhs);
                                        }
                                        None => {
                                            // Substitute a place of a value with an expression
                                            let rhs_expr =
                                                self.mir_encoder.encode_operand_expr(operand);
                                            let value_field =
                                                self.encoder.encode_value_field(field_ty);
                                            state.substitute_value(
                                                &field_place.field(value_field),
                                                rhs_expr,
                                            );
                                        }
                                    }
                                }
                            }

                            ref x => unimplemented!("{:?}", x),
                        }
                    }

                    &mir::Rvalue::BinaryOp(op, ref left, ref right) => {
                        let encoded_left = self.mir_encoder.encode_operand_expr(left);
                        let encoded_right = self.mir_encoder.encode_operand_expr(right);
                        let encoded_value = self.mir_encoder.encode_bin_op_expr(
                            op,
                            encoded_left,
                            encoded_right,
                            ty,
                        );

                        // Substitute a place of a value with an expression
                        state.substitute_value(&opt_lhs_value_place.unwrap(), encoded_value);
                    }

                    &mir::Rvalue::CheckedBinaryOp(op, ref left, ref right) => {
                        let operand_ty = if let ty::TypeVariants::TyTuple(ref types) = ty.sty {
                            types[0].clone()
                        } else {
                            unreachable!()
                        };

                        let encoded_left = self.mir_encoder.encode_operand_expr(left);
                        let encoded_right = self.mir_encoder.encode_operand_expr(right);

                        let encoded_value = self.mir_encoder.encode_bin_op_expr(
                            op,
                            encoded_left.clone(),
                            encoded_right.clone(),
                            operand_ty,
                        );
                        let encoded_check = self.mir_encoder.encode_bin_op_check(
                            op,
                            encoded_left,
                            encoded_right,
                            operand_ty,
                        );

                        let field_types = if let ty::TypeVariants::TyTuple(ref x) = ty.sty {
                            x
                        } else {
                            unreachable!()
                        };
                        let value_field = self
                            .encoder
                            .encode_raw_ref_field("tuple_0".to_string(), field_types[0]);
                        let value_field_value = self.encoder.encode_value_field(field_types[0]);
                        let check_field = self
                            .encoder
                            .encode_raw_ref_field("tuple_1".to_string(), field_types[1]);
                        let check_field_value = self.encoder.encode_value_field(field_types[1]);

                        let lhs_value = encoded_lhs
                            .clone()
                            .field(value_field)
                            .field(value_field_value);
                        let lhs_check = encoded_lhs
                            .clone()
                            .field(check_field)
                            .field(check_field_value);

                        // Substitute a place of a value with an expression
                        state.substitute_value(&lhs_value, encoded_value);
                        state.substitute_value(&lhs_check, encoded_check);
                    }

                    &mir::Rvalue::UnaryOp(op, ref operand) => {
                        let encoded_val = self.mir_encoder.encode_operand_expr(operand);
                        let encoded_value = self.mir_encoder.encode_unary_op_expr(op, encoded_val);

                        // Substitute a place of a value with an expression
                        state.substitute_value(&opt_lhs_value_place.unwrap(), encoded_value);
                    }

                    &mir::Rvalue::NullaryOp(_op, ref _op_ty) => unimplemented!(),

                    &mir::Rvalue::Discriminant(ref src) => {
                        let (encoded_src, src_ty, _) = self.mir_encoder.encode_place(src);
                        match src_ty.sty {
                            ty::TypeVariants::TyAdt(ref adt_def, _) if !adt_def.is_box() => {
                                let num_variants = adt_def.variants.len();

                                let discr_value: vir::Expr = if num_variants == 0 {
                                    let pos = self
                                        .encoder
                                        .error_manager()
                                        .register(stmt.source_info.span, ErrorCtxt::Unexpected);
                                    let function_name = self.encoder.encode_builtin_function_use(
                                        BuiltinFunctionKind::Unreachable(vir::Type::Int),
                                    );
                                    vir::Expr::func_app(
                                        function_name,
                                        vec![],
                                        vec![],
                                        vir::Type::Int,
                                        pos,
                                    )
                                } else {
                                    if num_variants == 1 {
                                        0.into()
                                    } else {
                                        let discr_field = self.encoder.encode_discriminant_field();
                                        encoded_src.field(discr_field).into()
                                    }
                                };

                                // Substitute a place of a value with an expression
                                state.substitute_value(&opt_lhs_value_place.unwrap(), discr_value);
                            }
                            ref x => {
                                panic!("The discriminant of type {:?} is not defined", x);
                            }
                        }
                    }

                    &mir::Rvalue::Ref(_, mir::BorrowKind::Unique, ref place)
                    | &mir::Rvalue::Ref(_, mir::BorrowKind::Mut { .. }, ref place)
                    | &mir::Rvalue::Ref(_, mir::BorrowKind::Shared, ref place) => {
                        let encoded_place = self.mir_encoder.encode_place(place).0;
                        let encoded_ref = match encoded_place {
                            vir::Expr::Field(
                                box ref base,
                                vir::Field { ref name, .. },
                                ref _pos,
                            ) if name == "val_ref" => {
                                // Simplify "address of reference"
                                base.clone()
                            }
                            other_place => other_place.addr_of(),
                        };

                        // Substitute the place
                        state.substitute_place(&encoded_lhs, encoded_ref);
                    }

                    &mir::Rvalue::Cast(mir::CastKind::Misc, ref operand, dst_ty) => {
                        let encoded_val = self.mir_encoder.encode_cast_expr(operand, dst_ty);

                        // Substitute a place of a value with an expression
                        state.substitute_value(&opt_lhs_value_place.unwrap(), encoded_val);
                    }

                    ref rhs => {
                        unimplemented!("encoding of '{:?}'", rhs);
                    }
                }
            }

            ref stmt => unimplemented!("encoding of '{:?}'", stmt),
        }
    }
}
