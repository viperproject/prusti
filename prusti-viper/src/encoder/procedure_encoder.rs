// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::encoder::borrows::ProcedureContract;
use crate::encoder::builtin_encoder::BuiltinMethodKind;
use crate::encoder::errors::PanicCause;
use crate::encoder::errors::{EncodingError, ErrorCtxt};
use crate::encoder::foldunfold;
use crate::encoder::places;
use crate::encoder::initialisation::InitInfo;
use crate::encoder::loop_encoder::{LoopEncoder, LoopEncoderError};
use crate::encoder::mir_encoder::{MirEncoder, FakeMirEncoder, PlaceEncoder};
use crate::encoder::mir_encoder::{POSTCONDITION_LABEL, PRECONDITION_LABEL};
use crate::encoder::mir_successor::MirSuccessor;
use crate::encoder::expiration_tool::{ExpirationTool, ExpirationToolCarrier};
use crate::encoder::expiration_tool::encode::binding::Binding;
use crate::encoder::optimizer;
use crate::encoder::places::{Local, LocalVariableManager, Place};
use crate::encoder::Encoder;
use crate::encoder::snapshot_spec_patcher::SnapshotSpecPatcher;
use prusti_common::{
    config,
    report::log,
    utils::to_string::ToString,
    vir::{
        self,
        borrows::Borrow,
        collect_assigned_vars,
        fixes::fix_ghost_vars,
        optimizations::methods::{remove_empty_if, remove_trivial_assertions, remove_unused_vars},
        CfgBlockIndex, Expr, ExprIterator, FoldingBehaviour, Successor, Type,
    },
};
use prusti_interface::environment::mir_utils::PlaceAddProjection;
use prusti_interface::{
    data::ProcedureDefId,
    environment::{
        borrowck::facts,
        polonius_info::{
            LoanPlaces, PoloniusInfo, PoloniusInfoError, ReborrowingDAG, ReborrowingDAGNode,
            ReborrowingKind, ReborrowingZombity,
        },
        BasicBlockIndex, PermissionKind, Procedure,
    },
};
use prusti_interface::utils;
// use prusti_common::report::log;
// use prusti_interface::specifications::*;
use rustc_middle::mir::Mutability;
use rustc_middle::mir;
use rustc_middle::mir::TerminatorKind;
use rustc_middle::ty;
use rustc_middle::ty::layout;
use rustc_target::abi::Integer;
use rustc_middle::ty::layout::IntegerExt;
use rustc_index::vec::Idx;
// use rustc_data_structures::indexed_vec::Idx;
// use std;
use std::collections::HashMap;
use std::collections::HashSet;
use rustc_attr::IntType::SignedInt;
// use syntax::codemap::{MultiSpan, Span};
use rustc_span::{MultiSpan, Span};
use prusti_interface::specs::typed;
use ::log::{trace, debug, error};
use prusti_common::vir::Position;
use prusti_interface::specs::typed::{AssertionKind, SpecificationSet};
use prusti_specs::specifications::common::ProcedureSpecification;
use crate::utils::namespace::Namespace;
use std::borrow::Borrow as StdBorrow;

pub type Result<T> = std::result::Result<T, EncodingError>;

pub struct ProcedureEncoder<'p, 'v: 'p, 'tcx: 'v> {
    pub encoder: &'p Encoder<'v, 'tcx>,
    proc_def_id: ProcedureDefId,
    pub procedure: &'p Procedure<'p, 'tcx>,
    pub mir: &'p mir::Body<'tcx>,
    pub cfg_method: vir::CfgMethod,
    locals: LocalVariableManager<'tcx>,
    loop_encoder: LoopEncoder<'p, 'tcx>,
    auxiliary_local_vars: HashMap<String, vir::Type>,
    pub mir_encoder: MirEncoder<'p, 'v, 'tcx>,
    check_panics: bool,
    check_foldunfold_state: bool,
    polonius_info: Option<PoloniusInfo<'p, 'tcx>>,
    pub procedure_contract: Option<ProcedureContract<'tcx>>,
    label_after_location: HashMap<mir::Location, String>,
    // /// Store the CFG blocks that encode a MIR block each.
    cfg_blocks_map: HashMap<mir::BasicBlock, HashSet<CfgBlockIndex>>,
    // /// Contains the boolean local variables that became `true` the first time the block is executed
    cfg_block_has_been_executed: HashMap<mir::BasicBlock, vir::LocalVar>,
    call_labels: HashMap<mir::Location, (String, String)>,
    // /// Contracts of functions called at given locations with map for replacing fake expressions.
    pub procedure_contracts:
        HashMap<mir::Location, (ProcedureContract<'tcx>, HashMap<vir::Expr, vir::Expr>)>,
    // /// A map that stores local variables used to preserve the value of a place accross the loop
    // /// when we cannot do that by using permissions.
    pure_var_for_preserving_value_map: HashMap<BasicBlockIndex, HashMap<vir::Expr, vir::LocalVar>>,
    /// Information about which places are definitely initialised.
    init_info: InitInfo,
    // /// Mapping from old expressions to ghost variables with which they were replaced.
    old_to_ghost_var: HashMap<vir::Expr, vir::Expr>,
    /// Ghost variables used inside package statements.
    old_ghost_vars: HashMap<String, vir::Type>,
    /// For each loop head, the block at whose end the loop invariant holds
    cached_loop_invariant_block: HashMap<BasicBlockIndex, BasicBlockIndex>,
}

impl<'p, 'v: 'p, 'tcx: 'v> ProcedureEncoder<'p, 'v, 'tcx> {
    pub fn new(encoder: &'p Encoder<'v, 'tcx>, procedure: &'p Procedure<'p, 'tcx>) -> Result<Self> {
        debug!("ProcedureEncoder constructor");

        let mir = procedure.get_mir();
        let def_id = procedure.get_id();
        let tcx = encoder.env().tcx();
        let mir_encoder = MirEncoder::new(encoder, mir, def_id);
        let init_info = match InitInfo::new(mir, tcx, def_id, &mir_encoder) {
            Ok(result) => result,
            _ => {
                return Err(EncodingError::unsupported(
                    format!("cannot encode {:?} because it uses unimplemented features",
                                    procedure.get_def_path()),
                    procedure.get_span()))
            }
        };

        let cfg_method = vir::CfgMethod::new(
            // method name
            encoder.encode_item_name(def_id),
            // formal args
            mir.arg_count,
            // formal returns
            vec![],
            // local vars
            vec![],
            // reserved labels
            vec![],
        );

        Ok(ProcedureEncoder {
            encoder,
            proc_def_id: def_id,
            procedure,
            mir,
            cfg_method,
            locals: LocalVariableManager::new(&mir.local_decls),
            loop_encoder: LoopEncoder::new(procedure, tcx),
            auxiliary_local_vars: HashMap::new(),
            mir_encoder: mir_encoder,
            check_panics: config::check_panics(),
            check_foldunfold_state: config::check_foldunfold_state(),
            polonius_info: None,
            procedure_contract: None,
            label_after_location: HashMap::new(),
            cfg_block_has_been_executed: HashMap::new(),
            cfg_blocks_map: HashMap::new(),
            call_labels: HashMap::new(),
            procedure_contracts: HashMap::new(),
            pure_var_for_preserving_value_map: HashMap::new(),
            init_info: init_info,
            old_to_ghost_var: HashMap::new(),
            old_ghost_vars: HashMap::new(),
            cached_loop_invariant_block: HashMap::new(),
        })
    }

    fn translate_polonius_error(&self, error: PoloniusInfoError) -> EncodingError {
        match error {
            PoloniusInfoError::UnsupportedLoanInLoop {
                loop_head,
                variable,
            } => {
                let msg = if self.mir.local_decls[variable].is_user_variable() {
                    format!("creation of loan 'FIXME: extract variable name' in loop is unsupported")
                } else {
                    "creation of temporary loan in loop is unsupported".to_string()
                };
                EncodingError::unsupported(msg, self.mir_encoder.get_span_of_basic_block(loop_head))
            }

            PoloniusInfoError::LoansInNestedLoops(location1, _loop1, _location2, _loop2) => {
                EncodingError::unsupported(
                    "creation of loans in nested loops is not supported".to_string(),
                    self.mir.source_info(location1).span,
                )
            }

            PoloniusInfoError::ReborrowingDagHasNoMagicWands(location) => {
                EncodingError::unsupported(
                    "the creation of loans in this loop is not yet supported \
                    (ReborrowingDagHasNoMagicWands)",
                    self.mir.source_info(location).span,
                )
            }

            PoloniusInfoError::MultipleMagicWandsPerLoop(location) => EncodingError::unsupported(
                "the creation of loans in this loop is not yet supported \
                    (MultipleMagicWandsPerLoop)",
                self.mir.source_info(location).span,
            ),

            PoloniusInfoError::MagicWandHasNoRepresentativeLoan(location) => {
                EncodingError::unsupported(
                    "the creation of loans in this loop is not yet supported \
                    (MagicWandHasNoRepresentativeLoan)",
                    self.mir.source_info(location).span,
                )
            }
        }
    }

    pub fn polonius_info(&self) -> &PoloniusInfo<'p, 'tcx> {
        self.polonius_info.as_ref().unwrap()
    }

    fn procedure_contract(&self) -> &ProcedureContract<'tcx> {
        self.procedure_contract.as_ref().unwrap()
    }

    fn mut_contract(&mut self) -> &mut ProcedureContract<'tcx> {
        self.procedure_contract.as_mut().unwrap()
    }

    pub fn encode(mut self) -> Result<vir::CfgMethod> {
        trace!("Encode procedure {}", self.cfg_method.name());
        let mir_span = self.mir.span;

        // Retrieve the contract
        self.procedure_contract = Some(
            self.encoder
                .get_procedure_contract_for_def(self.proc_def_id),
        );

        // Prepare assertions to check specification refinement
        let mut precondition_weakening: Option<typed::Assertion> = None;
        let mut postcondition_strengthening: Option<typed::Assertion> = None;
        debug!("procedure_contract: {:?}", self.procedure_contract());
        //trace!("def_id of proc: {:?}", &self.proc_def_id);
        let impl_def_id = self.encoder.env().tcx().impl_of_method(self.proc_def_id);
    //     //trace!("def_id of impl: {:?}", &impl_def_id);
        if let Some(id) = impl_def_id {
            let def_id_trait = self.encoder.env().tcx().trait_id_of_impl(id);
            trace!("def_id of trait: {:?}", &def_id_trait);
            // Trait implementation method refinement
            // Choosing alternative C as discussed in
            // https://ethz.ch/content/dam/ethz/special-interest/infk/chair-program-method/pm/documents/Education/Theses/Matthias_Erdin_MA_report.pdf
            // pp 19-23
            if let Some(id) = def_id_trait {
                let proc_name = self
                    .encoder
                    .env()
                    .tcx()
                    .item_name(self.proc_def_id);
                    // .as_symbol();
                if let Some(assoc_item) = self.encoder.env().get_assoc_item(id, proc_name) {
                    // TODO use the impl's specs if there are any (separately replace pre/post!)
                    let procedure_trait_contract = self
                        .encoder
                        .get_procedure_contract_for_def(assoc_item.def_id);
                    let (mut proc_pre_specs, mut proc_post_specs, mut proc_pledge_specs) = {
                        if let typed::SpecificationSet::Procedure(typed::ProcedureSpecification{pres, posts, pledges}) =
                            &mut self.mut_contract().specification
                        {
                            (pres, posts, pledges)
                        } else {
                            unreachable!("Unexpected: {:?}", procedure_trait_contract.specification)
                        }
                    };

                    if proc_pre_specs.is_empty() {
                        proc_pre_specs
                            .extend_from_slice(procedure_trait_contract.functional_precondition())
                    } else {
                        let proc_pre = typed::Assertion {
                            kind: box typed::AssertionKind::And(
                                proc_pre_specs.clone()
                            ),
                        };
                        let proc_trait_pre = typed::Assertion {
                            kind: box typed::AssertionKind::And(
                                procedure_trait_contract
                                    .functional_precondition()
                                    .iter()
                                    .cloned()
                                    .collect(),
                            ),
                        };
                        precondition_weakening = Some(typed::Assertion {
                            kind: box typed::AssertionKind::Implies(proc_trait_pre, proc_pre),
                        });
                    }

                    if proc_post_specs.is_empty() && proc_pledge_specs.is_empty() {
                        proc_post_specs
                            .extend_from_slice(procedure_trait_contract.functional_postcondition());
                        proc_pledge_specs
                            .extend_from_slice(procedure_trait_contract.pledges());
                    } else {
                        if !proc_pledge_specs.is_empty() {
                            unimplemented!("Refining specifications with pledges is not supported");
                        }
                        let proc_post = typed::Assertion {
                            kind: box typed::AssertionKind::And(
                                proc_post_specs.clone()
                            ),
                        };
                        let proc_trait_post = typed::Assertion {
                            kind: box typed::AssertionKind::And(
                                procedure_trait_contract
                                    .functional_postcondition()
                                    .iter()
                                    .cloned()
                                    .collect(),
                            ),
                        };
                        postcondition_strengthening = Some(typed::Assertion {
                            kind: box typed::AssertionKind::Implies(proc_post, proc_trait_post),
                        });
                    }
                }
            }
        }

        // Declare the formal return
        for local in self.mir.local_decls.indices().take(1) {
            let name = self.mir_encoder.encode_local_var_name(local);
            let type_name = self
                .encoder
                .encode_type_predicate_use(self.mir_encoder.get_local_ty(local)).unwrap(); // will panic if attempting to encode unsupported type
            self.cfg_method
                .add_formal_return(&name, vir::Type::TypedRef(type_name))
        }

        // Preprocess loops
        for bbi in self.procedure.get_reachable_nonspec_cfg_blocks() {
            if self.loop_encoder.loops().is_loop_head(bbi) {
                match self.loop_encoder.get_loop_invariant_block(bbi) {
                    Err(LoopEncoderError::LoopInvariantInBranch(loop_head)) => {
                        return Err(EncodingError::incorrect(
                            "the loop invariant cannot be in a conditional branch of the loop",
                            self.get_loop_span(loop_head),
                        ));
                    }
                    Ok(loop_inv_bbi) => {
                        self.cached_loop_invariant_block.insert(bbi, loop_inv_bbi);
                    }
                }
            }
        }

        // Load Polonius info
        self.polonius_info = Some(
            PoloniusInfo::new(&self.procedure, &self.cached_loop_invariant_block)
                .map_err(|err| self.translate_polonius_error(err))?,
        );

        // Initialize CFG blocks
        let start_cfg_block = self.cfg_method.add_block(
            "start",
            vec![],
            vec![
                vir::Stmt::comment("========== start =========="),
                // vir::Stmt::comment(format!("Name: {:?}", self.procedure.get_name())),
                vir::Stmt::comment(format!("Def path: {:?}", self.procedure.get_def_path())),
                vir::Stmt::comment(format!("Span: {:?}", self.procedure.get_span())),
            ],
        );

        let return_cfg_block = self.cfg_method.add_block(
            "return",
            vec![],
            vec![
                vir::Stmt::comment(format!("========== return ==========")),
                vir::Stmt::comment("Target of any 'return' statement."),
            ],
        );
        self.cfg_method
            .set_successor(return_cfg_block, Successor::Return);

        // Encode a flag that becomes true the first time a block is executed
        for bbi in self.procedure.get_reachable_nonspec_cfg_blocks() {
            let executed_flag_var = self.cfg_method.add_fresh_local_var(vir::Type::Bool);
            let bb_pos = self
                .mir_encoder
                .encode_expr_pos(self.mir_encoder.get_span_of_basic_block(bbi));
            self.cfg_method.add_stmt(
                start_cfg_block,
                vir::Stmt::Assign(
                    vir::Expr::local(executed_flag_var.clone()).set_pos(bb_pos),
                    false.into(),
                    vir::AssignKind::Copy,
                ),
            );
            self.cfg_block_has_been_executed
                .insert(bbi, executed_flag_var);
        }

        // Encode all blocks
        let (opt_body_head, unresolved_edges) = self.encode_blocks_group(
            "",
            &self.procedure.get_reachable_nonspec_cfg_blocks(),
            0,
            return_cfg_block,
        )?;
        if !unresolved_edges.is_empty() {
            return Err(EncodingError::internal(
                format!(
                    "there are unresolved CFG edges in the encoding: {:?}",
                    unresolved_edges
                ),
                mir_span,
            ));
        }

        // Set the first CFG block
        self.cfg_method.set_successor(
            start_cfg_block,
            Successor::Goto(opt_body_head.unwrap_or(return_cfg_block)),
        );

        // Encode preconditions
        self.encode_preconditions(start_cfg_block, precondition_weakening);

        // Encode postcondition
        self.encode_postconditions(return_cfg_block, postcondition_strengthening);

        let local_vars: Vec<_> = self
            .locals
            .iter()
            .filter(|local| !self.locals.is_return(*local))
            .collect();
        for local in local_vars.iter() {
            let local_ty = self.locals.get_type(*local);
            if let ty::TyKind::Closure(..) = local_ty.kind() {
                // Do not encode closures
                continue;
            }
            let type_name = self.encoder.encode_type_predicate_use(local_ty).unwrap(); // will panic if attempting to encode unsupported type
            let var_name = self.locals.get_name(*local);
            self.cfg_method
                .add_local_var(&var_name, vir::Type::TypedRef(type_name));
        }

        self.check_vir()?;
        let method_name = self.cfg_method.name();
        let source_path = self.encoder.env().source_path();
        let source_filename = source_path.file_name().unwrap().to_str().unwrap();

        self.encoder
            .log_vir_program_before_foldunfold(self.cfg_method.to_string());

        // Dump initial CFG
        if config::dump_debug_info() {
            prusti_common::report::log::report_with_writer(
                "graphviz_method_before_foldunfold",
                format!("{}.{}.dot", source_filename, method_name),
                |writer| self.cfg_method.to_graphviz(writer),
            );
        }

        // Add fold/unfold
        let loan_locations = self
            .polonius_info()
            .loan_locations()
            .iter()
            .map(|(loan, location)| (loan.into(), *location))
            .collect();
        let method_pos = self
            .encoder
            .error_manager()
            .register(self.mir.span, ErrorCtxt::Unexpected);
        let method_with_fold_unfold = foldunfold::add_fold_unfold(
            self.encoder,
            self.cfg_method,
            &loan_locations,
            &self.cfg_blocks_map,
            method_pos,
        )
        .map_err(|foldunfold_error| {
            EncodingError::internal(
                format!(
                    "generating fold-unfold Viper statements failed ({:?})",
                    foldunfold_error
                ),
                mir_span,
            )
        })?;

        // Fix variable declarations.
        let fixed_method = fix_ghost_vars(method_with_fold_unfold);

        // Do some optimizations
        let final_method = if config::simplify_encoding() {
            optimizer::rewrite(remove_trivial_assertions(remove_unused_vars(
                remove_empty_if(fixed_method),
            )))
        } else {
            fixed_method
        };

        // Dump final CFG
        if config::dump_debug_info() {
            prusti_common::report::log::report_with_writer(
                "graphviz_method_before_viper",
                format!("{}.{}.dot", source_filename, method_name),
                |writer| final_method.to_graphviz(writer),
            );
        }

        Ok(final_method)
    }

    /// Encodes a topologically ordered group of blocks.
    ///
    /// Returns:
    /// * The first CFG block of the encoding.
    /// * A vector of unresolved edges.
    fn encode_blocks_group(
        &mut self,
        label_prefix: &str,
        ordered_group_blocks: &[BasicBlockIndex],
        group_loop_depth: usize,
        return_block: CfgBlockIndex,
    ) -> Result<(Option<CfgBlockIndex>, Vec<(CfgBlockIndex, BasicBlockIndex)>)> {
        // Encode the CFG blocks
        let mut bb_map: HashMap<_, _> = HashMap::new();
        let mut unresolved_edges: Vec<_> = vec![];
        for &curr_bb in ordered_group_blocks.iter() {
            let loop_info = self.loop_encoder.loops();
            let curr_loop_depth = loop_info.get_loop_depth(curr_bb);
            let (curr_block, curr_edges) = if curr_loop_depth == group_loop_depth {
                // This block is not in a nested loop
                self.encode_block(label_prefix, curr_bb, return_block)?
            } else {
                debug_assert!(curr_loop_depth > group_loop_depth);
                let is_loop_head = loop_info.is_loop_head(curr_bb);
                if curr_loop_depth == group_loop_depth + 1 && is_loop_head {
                    // Encode a nested loop
                    self.encode_loop(label_prefix, curr_bb, return_block)?
                } else {
                    debug_assert!(curr_loop_depth > group_loop_depth + 1 || !is_loop_head);
                    // Skip the inner block of a nested loop
                    continue;
                }
            };
            bb_map.insert(curr_bb, curr_block);
            unresolved_edges.extend(curr_edges);
        }

        // Return unresolved CFG edges
        let group_head = ordered_group_blocks.get(0).map(|bb| {
            debug_assert!(
                bb_map.contains_key(bb),
                "Block {:?} (depth: {}, loop head: {}) has not been encoded \
                (group_loop_depth: {}, ordered_group_blocks: {:?})",
                bb,
                self.loop_encoder.loops().get_loop_depth(*bb),
                self.loop_encoder.loops().is_loop_head(*bb),
                group_loop_depth,
                ordered_group_blocks,
            );
            bb_map[bb]
        });
        let still_unresolved_edges =
            self.encode_unresolved_edges(unresolved_edges, |bb| bb_map.get(&bb).cloned())?;
        Ok((group_head, still_unresolved_edges))
    }

    fn encode_unresolved_edges<F: Fn(BasicBlockIndex) -> Option<CfgBlockIndex>>(
        &mut self,
        mut unresolved_edges: Vec<(CfgBlockIndex, BasicBlockIndex)>,
        resolver: F,
    ) -> Result<Vec<(CfgBlockIndex, BasicBlockIndex)>> {
        let mut still_unresolved_edges: Vec<_> = vec![];
        for (curr_block, target) in unresolved_edges.drain(..) {
            if let Some(target_block) = resolver(target) {
                self.cfg_method
                    .set_successor(curr_block, Successor::Goto(target_block));
            } else {
                still_unresolved_edges.push((curr_block, target));
            }
        }
        Ok(still_unresolved_edges)
    }

    /// Encodes a loop.
    ///
    /// Returns:
    /// * The first CFG block of the encoding
    /// * A vector of unresolved CFG edges
    ///
    /// The encoding transforms
    /// ```text
    /// while { g = G; g } { B1; invariant!(I); B2 }
    /// ```
    /// into
    /// ```text
    /// g = G
    /// if (g) {
    ///   B1
    ///   exhale I
    ///   // ... havoc local variables modified in G, B1, or B2
    ///   inhale I
    ///   B2
    ///   g = G
    ///   if (g) {
    ///     B1
    ///     exhale I
    ///     assume false
    ///   }
    /// }
    /// assume !g
    /// ```
    fn encode_loop(
        &mut self,
        label_prefix: &str,
        loop_head: BasicBlockIndex,
        return_block: CfgBlockIndex,
    ) -> Result<(CfgBlockIndex, Vec<(CfgBlockIndex, BasicBlockIndex)>)> {
        let loop_info = self.loop_encoder.loops();
        debug_assert!(loop_info.is_loop_head(loop_head));
        trace!("encode_loop: {:?}", loop_head);
        debug_assert!(loop_info.is_loop_head(loop_head));
        let loop_label_prefix = format!("{}loop{}", label_prefix, loop_head.index());
        let loop_depth = loop_info.get_loop_head_depth(loop_head);

        let loop_body: Vec<BasicBlockIndex> = loop_info
            .get_loop_body(loop_head)
            .iter()
            .filter(|&&bb| !self.procedure.is_spec_block(bb))
            .cloned()
            .collect();

        // Identify important blocks
        let loop_exit_blocks = loop_info.get_loop_exit_blocks(loop_head);
        let loop_exit_blocks_set: HashSet<_> = loop_exit_blocks.iter().cloned().collect();
        let before_invariant_block: BasicBlockIndex = self.cached_loop_invariant_block[&loop_head];
        let before_inv_block_pos = loop_body
            .iter()
            .position(|&bb| bb == before_invariant_block)
            .unwrap();
        let after_inv_block_pos = 1 + before_inv_block_pos;
        let exit_blocks_before_inv: Vec<_> = loop_body[0..after_inv_block_pos]
            .iter()
            .filter(|&bb| loop_exit_blocks_set.contains(bb))
            .cloned()
            .collect();
        // Heuristic: pick the first exit block before the invariant.
        // An infinite loop will have no exit blocks, so we have to use an Option here
        let opt_loop_guard_switch = exit_blocks_before_inv.last().cloned();
        let after_guard_block_pos = opt_loop_guard_switch
            .and_then(|loop_guard_switch| {
                loop_body
                    .iter()
                    .position(|&bb| bb == loop_guard_switch)
                    .map(|x| x + 1)
            })
            .unwrap_or(0);
        let after_guard_block = loop_body[after_guard_block_pos];
        let after_inv_block = loop_body[after_inv_block_pos];

        trace!("opt_loop_guard_switch: {:?}", opt_loop_guard_switch);
        trace!("before_invariant_block: {:?}", before_invariant_block);
        trace!("after_guard_block: {:?}", after_guard_block);
        trace!("after_inv_block: {:?}", after_inv_block);
        if loop_info.is_conditional_branch(loop_head, before_invariant_block) {
            debug!(
                "{:?} is conditional branch in loop {:?}",
                before_invariant_block, loop_head
            );
            let loop_head_span = self.mir_encoder.get_span_of_basic_block(loop_head);
            return Err(EncodingError::incorrect(
                "the loop invariant cannot be in a conditional branch of the loop",
                loop_body
                    .iter()
                    .map(|&bb| self.mir_encoder.get_span_of_basic_block(bb))
                    .filter(|&span| span.contains(loop_head_span))
                    .min()
                    .unwrap(),
            ));
        }

        // Split the blocks such that:
        // * G is loop_guard_evaluation, starting (if nonempty) with loop_head
        // * B1 is loop_body_before_inv, starting with after_guard_block (which could be loop_head)
        // * B2 is loop_body_after_inv, starting with after_inv_block
        let loop_guard_evaluation = &loop_body[0..after_guard_block_pos];
        let loop_body_before_inv = &loop_body[after_guard_block_pos..after_inv_block_pos];
        let loop_body_after_inv = &loop_body[after_inv_block_pos..];

        // The main path in the encoding is: start -> G -> B1 -> invariant -> B2 -> G -> B1 -> end
        // We are going to build the encoding left to right.
        let mut heads = vec![];

        // Build the "start" CFG block (*start* - G - B1 - invariant - B2 - G - B1 - end)
        let start_block = self.cfg_method.add_block(
            &format!("{}_start", loop_label_prefix),
            vec![],
            vec![vir::Stmt::comment(format!(
                "========== {}_start ==========",
                loop_label_prefix
            ))],
        );
        heads.push(Some(start_block));

        // Encode the first G group (start - *G* - B1 - invariant - B2 - G - B1 - end)
        let (first_g_head, first_g_edges) = self.encode_blocks_group(
            &format!("{}_group1_", loop_label_prefix),
            loop_guard_evaluation,
            loop_depth,
            return_block,
        )?;
        heads.push(first_g_head);

        // Encode the first B1 group (start - G - *B1* - invariant - B2 - G - B1 - end)
        let (first_b1_head, first_b1_edges) = self.encode_blocks_group(
            &format!("{}_group2_", loop_label_prefix),
            loop_body_before_inv,
            loop_depth,
            return_block,
        )?;
        heads.push(first_b1_head);

        // Build the "invariant" CFG block (start - G - B1 - *invariant* - B2 - G - B1 - end)
        // (1) checks the loop invariant on entry
        // (2) havocs the invariant and the local variables.
        let inv_pre_block = self.cfg_method.add_block(
            &format!("{}_inv_pre", loop_label_prefix),
            vec![],
            vec![vir::Stmt::comment(format!(
                "========== {}_inv_pre ==========",
                loop_label_prefix
            ))],
        );
        let inv_post_block = self.cfg_method.add_block(
            &format!("{}_inv_post", loop_label_prefix),
            vec![],
            vec![vir::Stmt::comment(format!(
                "========== {}_inv_post ==========",
                loop_label_prefix
            ))],
        );
        heads.push(Some(inv_pre_block));
        self.cfg_method
            .set_successor(inv_pre_block, vir::Successor::Goto(inv_post_block));
        {
            let stmts =
                self.encode_loop_invariant_exhale_stmts(loop_head, before_invariant_block, false);
            self.cfg_method.add_stmts(inv_pre_block, stmts);
        }
        // We'll add later more statements at the end of inv_pre_block, to havoc local variables
        {
            let stmts =
                self.encode_loop_invariant_inhale_stmts(loop_head, before_invariant_block, false);
            self.cfg_method.add_stmts(inv_post_block, stmts);
        }

        // Encode the last B2 group (start - G - B1 - invariant - *B2* - G - B1 - end)
        let (last_b2_head, last_b2_edges) = self.encode_blocks_group(
            &format!("{}_group3_", loop_label_prefix),
            loop_body_after_inv,
            loop_depth,
            return_block,
        )?;
        heads.push(last_b2_head);

        // Encode the last G group (start - G - B1 - invariant - B2 - *G* - B1 - end)
        let (last_g_head, last_g_edges) = self.encode_blocks_group(
            &format!("{}_group4_", loop_label_prefix),
            loop_guard_evaluation,
            loop_depth,
            return_block,
        )?;
        heads.push(last_g_head);

        // Encode the last B1 group (start - G - B1 - invariant - B2 - G - *B1* - end)
        let (last_b1_head, last_b1_edges) = self.encode_blocks_group(
            &format!("{}_group5_", loop_label_prefix),
            loop_body_before_inv,
            loop_depth,
            return_block,
        )?;
        heads.push(last_b1_head);

        // Build the "end" CFG block (start - G - B1 - invariant - B2 - G - B1 - *end*)
        // (1) checks the invariant after one loop iteration
        // (2) kills the program path with an `assume false`
        let end_body_block = self.cfg_method.add_block(
            &format!("{}_end_body", loop_label_prefix),
            vec![],
            vec![vir::Stmt::comment(format!(
                "========== {}_end_body ==========",
                loop_label_prefix
            ))],
        );
        {
            let stmts =
                self.encode_loop_invariant_exhale_stmts(loop_head, before_invariant_block, true);
            self.cfg_method.add_stmts(end_body_block, stmts);
        }
        self.cfg_method.add_stmt(
            end_body_block,
            vir::Stmt::Inhale(false.into(), vir::FoldingBehaviour::Stmt),
        );
        heads.push(Some(end_body_block));

        // We are going to link the unresolved edges.
        let mut still_unresolved_edges = vec![];

        // Link edges of "start" (*start* - G - B1 - invariant - B2 - G - B1 - end)
        let following_block = heads[1..].iter().find(|x| x.is_some()).unwrap().unwrap();
        self.cfg_method
            .set_successor(start_block, vir::Successor::Goto(following_block));

        // Link edges from the first G group (start - *G* - B1 - invariant - B2 - G - B1 - end)
        let following_block = heads[2..].iter().find(|x| x.is_some()).unwrap().unwrap();
        still_unresolved_edges.extend(self.encode_unresolved_edges(first_g_edges, |bb| {
            if bb == after_guard_block {
                Some(following_block)
            } else {
                None
            }
        })?);

        // Link edges from the first B1 group (start - G - *B1* - invariant - B2 - G - B1 - end)
        let following_block = heads[3..].iter().find(|x| x.is_some()).unwrap().unwrap();
        still_unresolved_edges.extend(self.encode_unresolved_edges(first_b1_edges, |bb| {
            if bb == after_inv_block {
                Some(following_block)
            } else {
                None
            }
        })?);

        // Link edges of "invariant" (start - G - B1 - *invariant* - B2 - G - B1 - end)
        let following_block = heads[4..].iter().find(|x| x.is_some()).unwrap().unwrap();
        self.cfg_method
            .set_successor(inv_post_block, vir::Successor::Goto(following_block));

        // Link edges from the last B2 group (start - G - B1 - invariant - *B2* - G - B1 - end)
        let following_block = heads[5..].iter().find(|x| x.is_some()).unwrap().unwrap();
        still_unresolved_edges.extend(self.encode_unresolved_edges(last_b2_edges, |bb| {
            if bb == loop_head {
                Some(following_block)
            } else {
                None
            }
        })?);

        // Link edges from the last G group (start - G - B1 - invariant - B2 - *G* - B1 - end)
        let following_block = heads[6..].iter().find(|x| x.is_some()).unwrap().unwrap();
        still_unresolved_edges.extend(self.encode_unresolved_edges(last_g_edges, |bb| {
            if bb == after_guard_block {
                Some(following_block)
            } else {
                None
            }
        })?);

        // Link edges from the last B1 group (start - G - B1 - invariant - B2 - G - *B1* - end)
        let following_block = heads[7..].iter().find(|x| x.is_some()).unwrap().unwrap();
        still_unresolved_edges.extend(self.encode_unresolved_edges(last_b1_edges, |bb| {
            if bb == after_inv_block {
                Some(following_block)
            } else {
                None
            }
        })?);

        // Link edges of "end" (start - G - B1 - invariant - B2 - G - B1 - *end*)
        self.cfg_method
            .set_successor(end_body_block, vir::Successor::Return);

        // Final step: havoc Viper local variables assigned in the encoding of the loop body
        let vars = collect_assigned_vars(&self.cfg_method, end_body_block, inv_pre_block);
        for var in vars {
            let builtin_method = match var.typ {
                vir::Type::Int => BuiltinMethodKind::HavocInt,
                vir::Type::Bool => BuiltinMethodKind::HavocBool,
                vir::Type::TypedRef(_) => BuiltinMethodKind::HavocRef,
                vir::Type::Domain(_) => BuiltinMethodKind::HavocRef,
            };
            let stmt = vir::Stmt::MethodCall(
                self.encoder.encode_builtin_method_use(builtin_method),
                vec![],
                vec![var],
            );
            self.cfg_method.add_stmt(inv_pre_block, stmt);
        }

        // Done. Phew!
        Ok((start_block, still_unresolved_edges))
}

    /// Encode a block.
    ///
    /// Returns:
    /// * The head of the encoded block
    /// * A vector unresolved edges
    fn encode_block(
        &mut self,
        label_prefix: &str,
        bbi: BasicBlockIndex,
        return_block: CfgBlockIndex,
    ) -> Result<(CfgBlockIndex, Vec<(CfgBlockIndex, BasicBlockIndex)>)> {
        debug_assert!(!self.procedure.is_spec_block(bbi));

        let curr_block = self.cfg_method.add_block(
            &format!("{}{:?}", label_prefix, bbi),
            vec![],
            vec![vir::Stmt::comment(format!(
                "========== {}{:?} ==========",
                label_prefix, bbi
            ))],
        );
        self.cfg_blocks_map
            .entry(bbi)
            .or_insert(HashSet::new())
            .insert(curr_block);

        if self.loop_encoder.is_loop_head(bbi) {
            self.cfg_method.add_stmt(
                curr_block,
                vir::Stmt::Comment("This is a loop head".to_string()),
            );
        }

        self.encode_execution_flag(bbi, curr_block)?;
        self.encode_block_statements(bbi, curr_block)?;
        let mir_successor: MirSuccessor = self.encode_block_terminator(bbi, curr_block)?;

        // Make sure that the
        let mir_targets = mir_successor.targets();
        // Force the encoding of a block if there is more than one successor, to leave
        // space for the fold-unfold algorithm.
        let force_block_on_edge = mir_targets.len() > 1;
        let mut targets_map = HashMap::new();
        let mut complete_resolution = true;
        for &target in &mir_targets {
            let opt_edge_block = self.encode_edge_block(bbi, target, force_block_on_edge)?;
            if let Some(edge_block) = opt_edge_block {
                targets_map.insert(target, edge_block);
            } else {
                complete_resolution = false;
            }
        }
        let unresolved_edges = if complete_resolution {
            // Resolve successor and return the edge blocks
            let curr_successor =
                mir_successor.encode(return_block, |target_bb| targets_map[&target_bb]);
            self.cfg_method.set_successor(curr_block, curr_successor);
            // This can be empty, if there are no unresolved edges left
            targets_map
                .iter()
                .map(|(&target, &edge_block)| (edge_block, target))
                .collect()
        } else {
            match mir_successor {
                MirSuccessor::Goto(target) => vec![(curr_block, target)],
                MirSuccessor::GotoSwitch(guarded_targets, default_target) => {
                    debug_assert!(guarded_targets.is_empty());
                    vec![(curr_block, default_target)]
                }
                x => unreachable!("{:?}", x),
            }
        };

        Ok((curr_block, unresolved_edges))
    }

    /// Store a flag that becomes true the first time the block is executed
    fn encode_execution_flag(
        &mut self,
        bbi: BasicBlockIndex,
        cfg_block: CfgBlockIndex,
    ) -> Result<()> {
        let pos = self
            .mir_encoder
            .encode_expr_pos(self.mir_encoder.get_span_of_basic_block(bbi));
        let executed_flag_var = self.cfg_block_has_been_executed[&bbi].clone();
        self.cfg_method.add_stmt(
            cfg_block,
            vir::Stmt::Assign(
                vir::Expr::local(executed_flag_var).set_pos(pos),
                true.into(),
                vir::AssignKind::Copy,
            ),
        );
        Ok(())
    }

    /// Encode the statements of the block
    fn encode_block_statements(
        &mut self,
        bbi: BasicBlockIndex,
        cfg_block: CfgBlockIndex,
    ) -> Result<()> {
        debug_assert!(!self.procedure.is_spec_block(bbi));
        let bb_data = &self.mir.basic_blocks()[bbi];
        let statements: &Vec<mir::Statement<'tcx>> = &bb_data.statements;
        let is_panic_block = self.procedure.is_panic_block(bbi);
        for stmt_index in 0..statements.len() {
            trace!("Encode statement {:?}:{}", bbi, stmt_index);
            let location = mir::Location {
                block: bbi,
                statement_index: stmt_index,
            };
            if !is_panic_block {
                let (stmts, opt_succ) = self.encode_statement_at(location)?;
                debug_assert!(opt_succ.is_none());
                self.cfg_method.add_stmts(cfg_block, stmts);
            }
            {
                let stmts = self.encode_expiring_borrows_at(location)?;
                self.cfg_method.add_stmts(cfg_block, stmts);
            }
        }
        Ok(())
    }

    /// Encode the terminator of the block
    fn encode_block_terminator(
        &mut self,
        bbi: BasicBlockIndex,
        curr_block: CfgBlockIndex,
    ) -> Result<MirSuccessor> {
        trace!("Encode terminator of {:?}", bbi);
        let bb_data = &self.mir.basic_blocks()[bbi];
        let location = mir::Location {
            block: bbi,
            statement_index: bb_data.statements.len(),
        };
        let (stmts, opt_mir_successor) = self.encode_statement_at(location)?;
        self.cfg_method.add_stmts(curr_block, stmts);
        Ok(opt_mir_successor.unwrap())
    }

    /// Encode a MIR statement or terminator.
    fn encode_statement_at(
        &mut self,
        location: mir::Location,
    ) -> Result<(Vec<vir::Stmt>, Option<MirSuccessor>)> {
        debug!("Encode location {:?}", location);
        let bb_data = &self.mir[location.block];
        let index = location.statement_index;
        if index < bb_data.statements.len() {
            let mir_stmt = &bb_data.statements[index];
            let stmts = self.encode_statement(mir_stmt, location);
            Ok((stmts, None))
        } else {
            let mir_term = bb_data.terminator();
            let (stmts, succ) = self.encode_terminator(mir_term, location)?;
            Ok((stmts, Some(succ)))
        }
    }

    fn encode_statement(
        &mut self,
        stmt: &mir::Statement<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        debug!(
            "Encode statement '{:?}', span: {:?}",
            stmt.kind, stmt.source_info.span
        );

        let mut stmts = vec![vir::Stmt::comment(format!("[mir] {:?}", stmt))];

        let encoding_stmts = match stmt.kind {
            mir::StatementKind::StorageLive(..)
            | mir::StatementKind::StorageDead(..)
            | mir::StatementKind::FakeRead(..)
            | mir::StatementKind::AscribeUserType(..)
            | mir::StatementKind::Coverage(..)
            | mir::StatementKind::Nop => vec![],

            mir::StatementKind::Assign(box (ref lhs, ref rhs)) => {
                // FIXME: the following line will panic if attempting to encode unsupported types.
                let (encoded_lhs, ty, _) = self.mir_encoder.encode_place(lhs).unwrap();
                match rhs {
                    &mir::Rvalue::Use(ref operand) => {
                        self.encode_assign_operand(&encoded_lhs, operand, location)
                    }
                    &mir::Rvalue::Aggregate(ref aggregate, ref operands) => self
                        .encode_assign_aggregate(&encoded_lhs, ty, aggregate, operands, location),
                    &mir::Rvalue::BinaryOp(op, ref left, ref right) => {
                        self.encode_assign_binary_op(op, left, right, encoded_lhs, ty, location)
                    }
                    &mir::Rvalue::CheckedBinaryOp(op, ref left, ref right) => self
                        .encode_assign_checked_binary_op(
                            op,
                            left,
                            right,
                            encoded_lhs,
                            ty,
                            location,
                        ),
                    &mir::Rvalue::UnaryOp(op, ref operand) => {
                        self.encode_assign_unary_op(op, operand, encoded_lhs, ty, location)
                    }
                    &mir::Rvalue::NullaryOp(op, ref op_ty) => {
                        self.encode_assign_nullary_op(op, op_ty, encoded_lhs, ty, location)
                    }
                    &mir::Rvalue::Discriminant(ref src) => {
                        self.encode_assign_discriminant(src, location, encoded_lhs, ty)
                    }
                    &mir::Rvalue::Ref(ref _region, mir_borrow_kind, ref place) => {
                        self.encode_assign_ref(mir_borrow_kind, place, location, encoded_lhs, ty)
                    }
                    &mir::Rvalue::Cast(mir::CastKind::Misc, ref operand, dst_ty) => {
                        self.encode_cast(operand, dst_ty, encoded_lhs, ty, location)
                    }
                    ref rhs => {
                        unimplemented!("encoding of '{:?}'", rhs);
                    }
                }
            }

            ref x => unimplemented!("{:?}", x),
        };
        stmts.extend(encoding_stmts);
        stmts
            .into_iter()
            .map(|s| {
                let expr_pos = self
                    .encoder
                    .error_manager()
                    .register(stmt.source_info.span, ErrorCtxt::GenericExpression);
                let stmt_pos = self
                    .encoder
                    .error_manager()
                    .register(stmt.source_info.span, ErrorCtxt::GenericStatement);
                s.set_default_expr_pos(expr_pos).set_default_pos(stmt_pos)
            })
            .collect()
    }

    /// Translate a borrowed place to a place that is currently usable
    fn translate_maybe_borrowed_place(
        &self,
        location: mir::Location,
        place: vir::Expr,
    ) -> vir::Expr {
        let (all_active_loans, _) = self.polonius_info().get_all_active_loans(location);
        let relevant_active_loan_places: Vec<_> = all_active_loans
            .iter()
            .flat_map(|p| self.polonius_info().get_loan_places(p))
            .filter(|loan_places| {
                let (_, encoded_source, _) = self.encode_loan_places(loan_places);
                place.has_prefix(&encoded_source)
            })
            .collect();
        if relevant_active_loan_places.len() == 1 {
            let loan_places = &relevant_active_loan_places[0];
            let (encoded_dest, encoded_source, _) = self.encode_loan_places(loan_places);
            // Recursive translation
            self.translate_maybe_borrowed_place(
                loan_places.location,
                place.replace_place(&encoded_source, &encoded_dest),
            )
        } else {
            place
        }
    }

    /// Encode the lhs and the rhs of the assignment that create the loan
    fn encode_loan_places(&self, loan_places: &LoanPlaces<'tcx>) -> (vir::Expr, vir::Expr, bool) {
        debug!("encode_loan_rvalue '{:?}'", loan_places);
        // will panic if attempting to encode unsupported type
        let (expiring_base, expiring_ty, _) = self.mir_encoder.encode_place(&loan_places.dest).unwrap();
        let encode = |rhs_place| {
            let (restored, _, _) = self.mir_encoder.encode_place(rhs_place).unwrap();
            let ref_field = self.encoder.encode_value_field(expiring_ty);
            let expiring = expiring_base.clone().field(ref_field.clone());
            (expiring, restored, ref_field)
        };
        match loan_places.source {
            mir::Rvalue::Ref(_, mir_borrow_kind, ref rhs_place) => {
                let (expiring, restored, _) = encode(rhs_place);
                assert_eq!(expiring.get_type(), restored.get_type());
                let is_mut = match mir_borrow_kind {
                    mir::BorrowKind::Shared => false,
                    mir::BorrowKind::Shallow => unimplemented!(),
                    mir::BorrowKind::Unique => unimplemented!(),
                    mir::BorrowKind::Mut { .. } => true,
                };
                (expiring, restored, is_mut)
            }
            mir::Rvalue::Use(mir::Operand::Move(ref rhs_place)) => {
                let (expiring, restored_base, ref_field) = encode(rhs_place);
                let restored = restored_base.clone().field(ref_field);
                assert_eq!(expiring.get_type(), restored.get_type());
                (expiring, restored, true)
            }
            mir::Rvalue::Use(mir::Operand::Copy(ref rhs_place)) => {
                let (expiring, restored_base, ref_field) = encode(rhs_place);
                let restored = restored_base.clone().field(ref_field);
                assert_eq!(expiring.get_type(), restored.get_type());
                (expiring, restored, false)
            }

            ref x => unreachable!("Borrow restores rvalue {:?}", x),
        }
    }

    pub fn encode_transfer_permissions(
        &mut self,
        lhs: vir::Expr,
        rhs: vir::Expr,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        let mut stmts = if let Some(var) = self.old_to_ghost_var.get(&rhs) {
            vec![vir::Stmt::Assign(
                var.clone(),
                lhs.clone(),
                vir::AssignKind::Move,
            )]
        } else {
            vec![vir::Stmt::TransferPerm(lhs.clone(), rhs.clone(), false)]
        };

        if self.check_foldunfold_state {
            let pos = self
                .encoder
                .error_manager()
                .register(self.mir.source_info(location).span, ErrorCtxt::Unexpected);
            stmts.push(vir::Stmt::Assert(
                vir::Expr::eq_cmp(lhs.clone().into(), rhs.into()),
                vir::FoldingBehaviour::Expr,
                pos,
            ));
        }

        stmts
    }

    pub fn encode_obtain(&mut self, expr: vir::Expr, pos: vir::Position) -> Vec<vir::Stmt> {
        let mut stmts = vec![];

        stmts.push(vir::Stmt::Obtain(expr.clone(), pos));

        if self.check_foldunfold_state {
            let pos = self.encoder.error_manager().register(
                // TODO: use a better span
                self.mir.span,
                ErrorCtxt::Unexpected,
            );
            stmts.push(vir::Stmt::Assert(expr, vir::FoldingBehaviour::Expr, pos));
        }

        stmts
    }

    /// A borrow is mutable if it was a MIR unique borrow, a move of
    /// a borrow, or a argument of a function.
    fn is_mutable_borrow(&self, loan: facts::Loan) -> bool {
        if let Some(stmt) = self.polonius_info().get_assignment_for_loan(loan) {
            match stmt.kind {
                mir::StatementKind::Assign(box (_, ref rhs)) => match rhs {
                    &mir::Rvalue::Ref(_, mir::BorrowKind::Shared, _) |
                    &mir::Rvalue::Use(mir::Operand::Copy(_)) => false,
                    &mir::Rvalue::Ref(_, mir::BorrowKind::Mut { .. }, _) |
                    &mir::Rvalue::Use(mir::Operand::Move(_)) => true,
                    x => unreachable!("{:?}", x),
                },
                ref x => unreachable!("{:?}", x),
            }
        } else {
            // It is not an assignment, so we assume that the borrow is mutable.
            true
        }
    }

    fn construct_vir_reborrowing_dag(
        &mut self,
        loans: &[facts::Loan],
        zombie_loans: &[facts::Loan],
        location: mir::Location,
        end_location: Option<mir::Location>,
    ) -> Result<vir::borrows::DAG> {
        let mir_dag = self
            .polonius_info()
            .construct_reborrowing_dag(&loans, &zombie_loans, location)
            .map_err(|err| self.translate_polonius_error(err))?;
        debug!(
            "construct_vir_reborrowing_dag mir_dag={}",
            mir_dag.to_string()
        );
        let mut expired_loans = Vec::new();
        let mut builder = vir::borrows::DAGBuilder::new();
        for node in mir_dag.iter() {
            let vir_node = match node.kind {
                ReborrowingKind::Assignment { loan } => self
                    .construct_vir_reborrowing_node_for_assignment(
                        &mir_dag,
                        loan,
                        node,
                        location,
                        end_location,
                    ),
                ReborrowingKind::Call { loan: expiring_loan, .. } =>
                    self.construct_vir_reborrowing_node_for_call(
                        &expired_loans, expiring_loan, node, location),
                ReborrowingKind::ArgumentMove { loan } => {
                    let loan_location = self.polonius_info().get_loan_location(&loan);
                    let guard = self.construct_location_guard(loan_location);
                    vir::borrows::Node::new(
                        guard,
                        node.loan.into(),
                        convert_loans_to_borrows(&node.reborrowing_loans),
                        convert_loans_to_borrows(&node.reborrowed_loans),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        None,
                    )
                }
                ref x => unimplemented!("{:?}", x),
            };
            expired_loans.push(node.loan);
            builder.add_node(vir_node);
        }
        debug!(
            "construct_vir_reborrowing_dag mir_dag={}",
            mir_dag.to_string()
        );
        Ok(builder.finish())
    }

    fn construct_location_guard(&self, location: mir::Location) -> vir::Expr {
        let bbi = &location.block;
        let executed_flag_var = self.cfg_block_has_been_executed[bbi].clone();
        vir::Expr::local(executed_flag_var).into()
    }

    fn construct_vir_reborrowing_node_for_assignment(
        &mut self,
        _mir_dag: &ReborrowingDAG,
        loan: facts::Loan,
        node: &ReborrowingDAGNode,
        location: mir::Location,
        end_location: Option<mir::Location>,
    ) -> vir::borrows::Node {
        let mut stmts: Vec<vir::Stmt> = Vec::new();
        let node_is_leaf = node.reborrowed_loans.is_empty();

        let loan_location = self.polonius_info().get_loan_location(&loan);
        let loan_places = self.polonius_info().get_loan_places(&loan).unwrap();
        let (expiring, restored, is_mut) = self.encode_loan_places(&loan_places);
        let borrowed_places = vec![restored.clone()];

        let mut used_lhs_label = false;

        // Move the permissions from the "in loans" ("reborrowing loans") to the current loan
        if node.incoming_zombies {
            let lhs_label = self.get_label_after_location(loan_location).to_string();
            for &in_loan in node.reborrowing_loans.iter() {
                if self.is_mutable_borrow(in_loan) {
                    let in_location = self.polonius_info().get_loan_location(&in_loan);
                    let in_label = self.get_label_after_location(in_location).to_string();
                    used_lhs_label = true;
                    stmts.extend(self.encode_transfer_permissions(
                        expiring.clone().old(&in_label),
                        expiring.clone().old(&lhs_label),
                        loan_location,
                    ));
                }
            }
        }

        let lhs_place = if used_lhs_label {
            let lhs_label = self.get_label_after_location(loan_location);
            expiring.clone().old(lhs_label)
        } else {
            expiring.clone()
        };
        let rhs_place = match node.zombity {
            ReborrowingZombity::Zombie(rhs_location) if !node_is_leaf => {
                let rhs_label = self.get_label_after_location(rhs_location);
                restored.clone().old(rhs_label)
            }

            _ => restored,
        };

        if is_mut {
            stmts.extend(self.encode_transfer_permissions(
                lhs_place.clone(),
                rhs_place,
                loan_location,
            ));
        }

        let conflicting_loans = self.polonius_info().get_conflicting_loans(node.loan);
        let deaf_location = if let Some(end_location) = end_location {
            end_location
        } else {
            location
        };
        let alive_conflicting_loans = self
            .polonius_info()
            .get_alive_conflicting_loans(node.loan, deaf_location);

        let guard = self.construct_location_guard(loan_location);
        vir::borrows::Node::new(
            guard,
            node.loan.into(),
            convert_loans_to_borrows(&node.reborrowing_loans),
            convert_loans_to_borrows(&node.reborrowed_loans),
            stmts,
            borrowed_places,
            convert_loans_to_borrows(&conflicting_loans),
            convert_loans_to_borrows(&alive_conflicting_loans),
            Some(lhs_place.clone()),
        )
    }

    /// * `expired_loan` is the loan that is expiring.
    /// * `node` is the re-borrowing node associated with this loan.
    /// * `location` is the location where the loan expires.
    fn construct_vir_reborrowing_node_for_call(
        &mut self,
        expired_loans: &[facts::Loan],
        expiring_loan: facts::Loan,
        node: &ReborrowingDAGNode,
        location: mir::Location,
    ) -> vir::borrows::Node {
        // Collect some useful data.
        let tcx = self.encoder.env().tcx();

        let call_location = self.polonius_info().get_loan_location(&expiring_loan);
        let (contract, _) = &self.procedure_contracts[&call_location].clone();

        let reborrow_signature = &contract.borrow_infos;

        let def_id = ty::WithOptConstParam::unknown(contract.def_id.expect_local());
        let (mir, _) = tcx.mir_promoted(def_id);

        let pledges = contract.pledges().iter().map(|pledge| pledge.rhs.clone()).collect();

        let (active_loans, _) = self.polonius_info().get_all_active_loans(location);

        fn to_substituted_place(place: mir::Place) -> Place {
            Place::from_place(mir::RETURN_PLACE, place)
        }

        let expiring_place = self.polonius_info().get_loan_call_place(&expiring_loan).unwrap();
        let expiring_place = to_substituted_place(expiring_place.deref(tcx));

        let still_blocking = active_loans.iter()
            .filter_map(|loan| self.polonius_info().get_loan_call_place(loan))
            .map(|place| to_substituted_place(place.clone().deref(tcx)))
            .collect::<HashSet<_>>();

        let expired_before_1 = reborrow_signature.blocking.difference(&still_blocking)
            .collect::<HashSet<_>>();

        let expired_before_2 = expired_loans.iter()
            .filter_map(|loan| self.polonius_info().get_loan_call_place(loan))
            .map(|place| to_substituted_place(place.clone().deref(tcx)))
            .collect::<Vec<_>>();

        let expired_before = std::iter::empty()
            .chain(expired_before_1)
            .chain(expired_before_2.iter());

        // We construct the initial expiration tools.
        let mut carrier = ExpirationToolCarrier::default();
        let expiration_tool =
            carrier.construct(tcx, &mir.borrow(), reborrow_signature, pledges).unwrap();

        // And now we drill down into the expiration tools by expiring the places that have already
        // expired.
        let expiration_tool = expiration_tool.expire(expired_before);
        let magic_wand = expiration_tool.magic_wand(&expiring_place).unwrap();

        let (pre_label, post_label) = self.call_labels[&call_location].clone();
        let (encoded_magic_wand, materialized_bindings, open_bindings) =
            self.encode_magic_wand_as_expression(
                &magic_wand, contract, Some(call_location), &pre_label, &post_label);

        let encoded_magic_wand = open_bindings.into_iter().fold(
            encoded_magic_wand,
            |encoded_magic_wand, Binding(var, _, _)| {
                let reified_var_name = format!("{}$reified", var.name.clone());
                let reified_var = vir::LocalVar::new(reified_var_name, var.typ.clone());
                encoded_magic_wand.replace_place(
                    &vir::Expr::local(var),
                    &vir::Expr::local(reified_var))
            });

        let lhs_label = self.cfg_method.add_fresh_label();

        let materialized_bindings = materialized_bindings.into_iter()
            .map(|Binding(var, context, expr)| {
                let var = vir::LocalVar::new(format!("{}$reified", var.name), var.typ);
                let expr = self.replace_old_places_with_ghost_vars(None, expr);
                Binding(var, context, expr)
            })
            .collect::<Vec<_>>();

        let (materialized_variables, materialized_assignments) =
            self.encode_bindings_as_assignments(materialized_bindings, &lhs_label);

        for mv in materialized_variables {
            if self.cfg_method.get_all_vars().iter().find(|v| mv.name == v.name).is_none() {
                self.cfg_method.add_local_var(&mv.name, mv.typ);
            }
        }

        let encoded_magic_wand = self.replace_old_places_with_ghost_vars(None, encoded_magic_wand);

        let (encoded_expired, _, _) = self.encode_generic_place(
            self.proc_def_id, Some(call_location), magic_wand.expired());

        let encoded_old_expired = encoded_expired.clone().old(post_label);

        let transfer_perms = if !node.reborrowing_loans.is_empty() {
            node.reborrowing_loans.iter().map(|in_loan| {
                let in_location = self.polonius_info().get_loan_location(&in_loan);
                let in_label = self.get_label_after_location(in_location).to_string();
                let encoded_expired = encoded_expired.clone().old(in_label);
                self.encode_transfer_permissions(
                    encoded_expired, encoded_old_expired.clone(), call_location)
            }).flatten().collect()
        } else {
            self.encode_transfer_permissions(
                encoded_expired, encoded_old_expired.clone(), call_location)
        };

        let stmts = [
            &transfer_perms[..],
            &[vir!(inhale [encoded_magic_wand])],
            &[vir!(label lhs_label)],
            &[vir!(apply [encoded_magic_wand])],
            &materialized_assignments[..]
        ].concat();

        vir::borrows::Node::new(
            self.construct_location_guard(call_location),
            node.loan.into(),
            convert_loans_to_borrows(&node.reborrowing_loans),
            convert_loans_to_borrows(&node.reborrowed_loans),
            stmts,
            Vec::new(), Vec::new(), Vec::new(), None
        )
    }

    pub fn encode_expiration_of_loans(
        &mut self,
        loans: Vec<facts::Loan>,
        zombie_loans: &[facts::Loan],
        location: mir::Location,
        end_location: Option<mir::Location>,
    ) -> Result<Vec<vir::Stmt>> {
        trace!(
            "encode_expiration_of_loans '{:?}' '{:?}'",
            loans,
            zombie_loans
        );
        let mut stmts: Vec<vir::Stmt> = vec![];
        if loans.len() > 0 {
            let vir_reborrowing_dag =
                self.construct_vir_reborrowing_dag(&loans, &zombie_loans, location, end_location)?;
            stmts.push(vir::Stmt::ExpireBorrows(vir_reborrowing_dag));
        }
        Ok(stmts)
    }

    fn encode_expiring_borrows_between(
        &mut self,
        begin_loc: mir::Location,
        end_loc: mir::Location,
    ) -> Result<Vec<vir::Stmt>> {
        debug!(
            "encode_expiring_borrows_beteewn '{:?}' '{:?}'",
            begin_loc, end_loc
        );
        let (all_dying_loans, zombie_loans) = self
            .polonius_info()
            .get_all_loans_dying_between(begin_loc, end_loc);
        // FIXME: is 'end_loc' correct here? What about 'begin_loc'?
        self.encode_expiration_of_loans(all_dying_loans, &zombie_loans, begin_loc, Some(end_loc))
    }

    fn encode_expiring_borrows_at(&mut self, location: mir::Location) -> Result<Vec<vir::Stmt>> {
        debug!("encode_expiring_borrows_at '{:?}'", location);
        let (all_dying_loans, zombie_loans) = self.polonius_info().get_all_loans_dying_at(location);
        self.encode_expiration_of_loans(all_dying_loans, &zombie_loans, location, None)
    }

    fn encode_terminator(
        &mut self,
        term: &mir::Terminator<'tcx>,
        location: mir::Location,
    ) -> Result<(Vec<vir::Stmt>, MirSuccessor)> {
        debug!(
            "Encode terminator '{:?}', span: {:?}",
            term.kind, term.source_info.span
        );
        let mut stmts: Vec<vir::Stmt> = vec![vir::Stmt::comment(format!("[mir] {:?}", term.kind))];

        let result = match term.kind {
            TerminatorKind::Return => {
                // Package magic wands, if there is any
                stmts.extend(self.encode_package_end_of_method(
                    PRECONDITION_LABEL,
                    POSTCONDITION_LABEL,
                    location,
                )?);

                (stmts, MirSuccessor::Return)
            }

            TerminatorKind::Goto { target } => (stmts, MirSuccessor::Goto(target)),

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

                let mut cfg_targets: Vec<(vir::Expr, BasicBlockIndex)> = vec![];

                // Use a local variable for the discriminant (see issue #57)
                let discr_var = match switch_ty.kind() {
                    ty::TyKind::Bool => {
                        self.cfg_method.add_fresh_local_var(vir::Type::Bool)
                    }

                    ty::TyKind::Int(_)
                    | ty::TyKind::Uint(_)
                    | ty::TyKind::Char => {
                        self.cfg_method.add_fresh_local_var(vir::Type::Int)
                    }

                    ref x => unreachable!("{:?}", x),
                };
                let encoded_discr = self.mir_encoder.encode_operand_expr(discr);
                stmts.push(vir::Stmt::Assign(
                    discr_var.clone().into(),
                    if encoded_discr.is_place() {
                        self.translate_maybe_borrowed_place(location, encoded_discr)
                    } else {
                        encoded_discr
                    },
                    vir::AssignKind::Copy,
                ));

                let guard_is_bool = match switch_ty.kind() {
                    ty::TyKind::Bool => true,
                    _ => false
                };

                for (i, &value) in values.iter().enumerate() {
                    let target = targets[i as usize];
                    // Convert int to bool, if required
                    let viper_guard = match switch_ty.kind() {
                        ty::TyKind::Bool => {
                            if value == 0 {
                                // If discr is 0 (false)
                                vir::Expr::not(discr_var.clone().into())
                            } else {
                                // If discr is not 0 (true)
                                discr_var.clone().into()
                            }
                        }

                        ty::TyKind::Int(_)
                        | ty::TyKind::Uint(_)
                        | ty::TyKind::Char => vir::Expr::eq_cmp(
                            discr_var.clone().into(),
                            self.encoder.encode_int_cast(value, switch_ty),
                        ),

                        ref x => unreachable!("{:?}", x),
                    };
                    cfg_targets.push((viper_guard, target))
                }
                let mut default_target = targets[values.len()];
                let mut kill_default_target = false;

                // Is the target an unreachable block?
                if let mir::TerminatorKind::Unreachable = self.mir[default_target].terminator().kind
                {
                    stmts.push(vir::Stmt::comment(format!(
                        "Ignore default target {:?}, as the compiler marked it as unreachable.",
                        default_target
                    )));
                    kill_default_target = true;
                };

                // Is the target a specification block?
                if self.procedure.is_spec_block(default_target) {
                    stmts.push(vir::Stmt::comment(format!(
                        "Ignore default target {:?}, as it is only used by Prusti to type-check \
                        a loop invariant.",
                        default_target
                    )));
                    kill_default_target = true;
                };

                if kill_default_target {
                    // Use the last conditional target as default. We could also assume or assert
                    // that the switch is exhaustive and never hits the default.
                    let last_target = cfg_targets.pop().unwrap();
                    (stmts, MirSuccessor::GotoSwitch(cfg_targets, last_target.1))
                } else {
                    // Reorder the targets such that Silicon explores branches in the order that we want
                    if guard_is_bool && cfg_targets.len() == 1 {
                        let (target_guard, target) = cfg_targets.pop().unwrap();
                        let target_span = self.mir_encoder.get_span_of_basic_block(target);
                        let default_target_span = self.mir_encoder.get_span_of_basic_block(default_target);
                        if target_span > default_target_span {
                            let guard_pos = target_guard.pos();
                            cfg_targets = vec![(
                                target_guard.negate().set_pos(guard_pos),
                                default_target,
                            )];
                            default_target = target;
                        } else {
                            // Undo the pop
                            cfg_targets.push((target_guard, target));
                        }
                    }
                    (stmts, MirSuccessor::GotoSwitch(cfg_targets, default_target))
                }
            }

            TerminatorKind::Unreachable => {
                // Asserting `false` here does not work. See issue #158
                //let pos = self.encoder.error_manager().register(
                //    term.source_info.span,
                //    ErrorCtxt::UnreachableTerminator
                //);
                //stmts.push(
                //    vir::Stmt::Inhale(false.into())
                //);
                (stmts, MirSuccessor::Kill)
            }

            TerminatorKind::Abort => {
                let pos = self
                    .encoder
                    .error_manager()
                    .register(term.source_info.span, ErrorCtxt::AbortTerminator);
                stmts.push(vir::Stmt::Assert(
                    false.into(),
                    vir::FoldingBehaviour::Stmt,
                    pos,
                ));
                (stmts, MirSuccessor::Kill)
            }

            TerminatorKind::Drop { target, .. } => (stmts, MirSuccessor::Goto(target)),

            TerminatorKind::FalseEdge { real_target, .. } => {
                (stmts, MirSuccessor::Goto(real_target))
            }

            TerminatorKind::FalseUnwind { real_target, .. } => {
                (stmts, MirSuccessor::Goto(real_target))
            }

            TerminatorKind::DropAndReplace {
                target,
                place: ref lhs,
                ref value,
                ..
            } => {
                // will panic if attempting to encode unsupported type
                let (encoded_lhs, _, _) = self.mir_encoder.encode_place(lhs).unwrap();
                stmts.extend(self.encode_assign_operand(&encoded_lhs, value, location));
                (stmts, MirSuccessor::Goto(target))
            }

            TerminatorKind::Call {
                ref args,
                ref destination,
                func:
                    mir::Operand::Constant(box mir::Constant {
                        literal:
                            ty::Const {
                                ty,
                                val: _
                            },
                        ..
                    }),
                ..
            } => {
                if let ty::TyKind::FnDef(def_id, substs) = ty.kind() {
                    let self_ty = {
                        // If we are calling a trait method on a struct, self_ty
                        // is the struct.
                        let generics = self.encoder.env().tcx().generics_of(*def_id);
                        if generics.has_self {
                            Some(substs.type_at(0))
                        } else {
                            None
                        }
                    };

                    let def_id = *self.encoder.get_specification_def_id(def_id);
                    let full_func_proc_name: &str =
                        &self.encoder.env().tcx().def_path_str(def_id);
                        // &self.encoder.env().tcx().absolute_item_path_str(def_id);

                    let own_substs =
                        ty::List::identity_for_item(self.encoder.env().tcx(), def_id);

                    {
                        // FIXME: this is a hack to support generics. See issue #187.
                        let mut tymap_stack = self.encoder.typaram_repl.borrow_mut();
                        let mut tymap = HashMap::new();

                        for (kind1, kind2) in own_substs.iter().zip(substs.iter()) {
                            if let (
                                ty::subst::GenericArgKind::Type(ty1),
                                ty::subst::GenericArgKind::Type(ty2),
                            ) = (kind1.unpack(), kind2.unpack())
                            {
                                tymap.insert(ty1, ty2);
                            }
                        }
                        tymap_stack.push(tymap);
                    }

                    match full_func_proc_name {
                        "std::rt::begin_panic" | "std::panicking::begin_panic" => {
                            // This is called when a Rust assertion fails
                            // args[0]: message
                            // args[1]: position of failing assertions

                            // Example of args[0]: 'const "internal error: entered unreachable code"'
                            let panic_message = format!("{:?}", args[0]);

                            let panic_cause = self.mir_encoder.encode_panic_cause(
                                term.source_info
                            );
                            let pos = self
                                .encoder
                                .error_manager()
                                .register(
                                    term.source_info.span,
                                    ErrorCtxt::Panic(panic_cause)
                                );

                            if self.check_panics {
                                stmts.push(vir::Stmt::comment(format!(
                                    "Rust panic - {}",
                                    panic_message
                                )));
                                stmts.push(vir::Stmt::Assert(
                                    false.into(),
                                    vir::FoldingBehaviour::Stmt,
                                    pos,
                                ));
                            } else {
                                debug!("Absence of panic will not be checked")
                            }
                        }

                        "std::boxed::Box::<T>::new" => {
                            // This is the initialization of a box
                            // args[0]: value to put in the box
                            assert_eq!(args.len(), 1);

                            let &(ref target_place, _) = destination.as_ref().unwrap(); // will panic if attempting to encode unsupported type
                            let (dst, dest_ty, _) = self.mir_encoder.encode_place(target_place).unwrap();
                            let boxed_ty = dest_ty.boxed_ty();
                            let ref_field = self.encoder.encode_dereference_field(boxed_ty);

                            let box_content = dst.clone().field(ref_field.clone());

                            stmts.extend(
                                self.prepare_assign_target(
                                    dst,
                                    ref_field,
                                    location,
                                    vir::AssignKind::Move,
                                )
                            );

                            // Allocate `box_content`
                            stmts.extend(self.encode_havoc_and_allocation(&box_content));

                            // Initialize `box_content`
                            stmts.extend(self.encode_assign_operand(&box_content, &args[0], location));
                        }

                        "std::cmp::PartialEq::eq" |
                        "core::cmp::PartialEq::eq"
                            if args.len() == 2 &&
                                self.encoder.has_structural_eq_impl(
                                    self.mir_encoder.get_operand_ty(&args[0])
                                )
                        => {
                            debug!("Encoding call of PartialEq::eq");
                            stmts.extend(
                                self.encode_cmp_function_call(
                                    def_id,
                                    location,
                                    term.source_info.span,
                                    args,
                                    destination,
                                    vir::BinOpKind::EqCmp,
                                )
                            );
                        }

                        "std::cmp::PartialEq::ne" |
                        "core::cmp::PartialEq::ne"
                            if args.len() == 2 &&
                                self.encoder.has_structural_eq_impl(
                                    self.mir_encoder.get_operand_ty(&args[0])
                                )
                        => {
                            debug!("Encoding call of PartialEq::ne");
                            stmts.extend(
                                self.encode_cmp_function_call(
                                    def_id,
                                    location,
                                    term.source_info.span,
                                    args,
                                    destination,
                                    vir::BinOpKind::NeCmp,
                                )
                            );
                        }

                        _ => {
                            let is_pure_function =
                                self.encoder.env().has_prusti_attribute(def_id, "pure");
                            if is_pure_function {
                                let (function_name, _) = self.encoder.encode_pure_function_use(def_id);
                                debug!("Encoding pure function call '{}'", function_name);
                                assert!(destination.is_some());

                                let mut arg_exprs = vec![];
                                for operand in args.iter() {
                                    let arg_expr = self.mir_encoder.encode_operand_expr(operand);
                                    arg_exprs.push(arg_expr);
                                }

                                stmts.extend(self.encode_pure_function_call(
                                    location,
                                    term.source_info.span,
                                    args,
                                    destination,
                                    def_id,
                                ));
                            } else {
                                stmts.extend(self.encode_impure_function_call(
                                    location,
                                    term.source_info.span,
                                    args,
                                    destination,
                                    def_id,
                                    self_ty,
                                )?);
                            }
                        }
                    }

                    // FIXME: this is a hack to support generics. See issue #187.
                    {
                        let mut tymap_stack = self.encoder.typaram_repl.borrow_mut();
                        tymap_stack.pop();
                    }

                    if let &Some((_, target)) = destination {
                        (stmts, MirSuccessor::Goto(target))
                    } else {
                        // Encode unreachability
                        //stmts.push(
                        //    vir::Stmt::Inhale(false.into())
                        //);
                        (stmts, MirSuccessor::Kill)
                    }
                } else {
                    // Other kind of calls?
                    unimplemented!();
                }
            }

            TerminatorKind::Call { .. } => {
                // Other kind of calls?
                unimplemented!();
            }

            TerminatorKind::Assert {
                ref cond,
                expected,
                target,
                ref msg,
                ..
            } => {
                trace!("Assert cond '{:?}', expected '{:?}'", cond, expected);

                // Use local variables in the switch/if (see GitLab issue #57)
                let cond_var = self.cfg_method.add_fresh_local_var(vir::Type::Bool);
                stmts.push(vir::Stmt::Assign(
                    cond_var.clone().into(),
                    self.mir_encoder.encode_operand_expr(cond),
                    vir::AssignKind::Copy,
                ));

                let viper_guard = if expected {
                    cond_var.into()
                } else {
                    vir::Expr::not(cond_var.into())
                };

                // Check or assume the assertion
                stmts.push(vir::Stmt::comment(format!(
                    "Rust assertion: {}",
                    msg.description()
                )));
                if self.check_panics {
                    stmts.push(vir::Stmt::Assert(
                        viper_guard,
                        vir::FoldingBehaviour::Stmt,
                        self.encoder.error_manager().register(
                            term.source_info.span,
                            ErrorCtxt::AssertTerminator(msg.description().to_string()),
                        ),
                    ));
                } else {
                    stmts.push(vir::Stmt::comment("This assertion will not be checked"));
                    stmts.push(vir::Stmt::Inhale(viper_guard, vir::FoldingBehaviour::Stmt));
                };

                (stmts, MirSuccessor::Goto(target))
            }

            TerminatorKind::Resume
            | TerminatorKind::Yield { .. }
            | TerminatorKind::GeneratorDrop
            | TerminatorKind::InlineAsm { .. } => unimplemented!("{:?}", term.kind),
        };
        Ok(result)
    }

    fn encode_cmp_function_call(
        &mut self,
        called_def_id: ProcedureDefId,
        location: mir::Location,
        call_site_span: Span,
        args: &[mir::Operand<'tcx>],
        destination: &Option<(mir::Place<'tcx>, BasicBlockIndex)>,
        bin_op: vir::BinOpKind,
    ) -> Vec<vir::Stmt> {

        let arg_ty = self.mir_encoder.get_operand_ty(&args[0]);

        let snapshot = self.encoder.encode_snapshot(&arg_ty);
        if snapshot.is_defined() {

            let pos = self
                .encoder
                .error_manager()
                .register(call_site_span, ErrorCtxt::PureFunctionCall);

            let lhs = self.mir_encoder.encode_operand_expr(&args[0]);
            let rhs = self.mir_encoder.encode_operand_expr(&args[1]);

            let expr = match bin_op {
                vir::BinOpKind::EqCmp => snapshot.encode_equals(lhs, rhs, pos),
                vir::BinOpKind::NeCmp => snapshot.encode_not_equals(lhs, rhs, pos),
                _ => unreachable!()
            };

            let target_value = self.encode_pure_function_call_lhs_value(destination);
            let inhaled_expr = vir::Expr::eq_cmp(target_value.into(), expr);

            let (mut stmts, label) = self.encode_pure_function_call_site(
                location,
                destination,
                inhaled_expr
            );

            self.encode_transfer_args_permissions(location, args,  &mut stmts, label);

            stmts
        } else {
            // the equality check involves some unsupported feature;
            // treat it as any other function
            self.encode_impure_function_call(
                location,
                call_site_span,
                args,
                destination,
                called_def_id,
                None, // FIXME: This is almost definitely wrong.
            ).ok().unwrap() // TODO CMFIXME return proper result
        }
    }

    /// Encode an edge of the MIR graph
    fn encode_edge_block(
        &mut self,
        source: BasicBlockIndex,
        destination: BasicBlockIndex,
        force_block: bool,
    ) -> Result<Option<CfgBlockIndex>> {
        let source_loc = mir::Location {
            block: source,
            statement_index: self.mir[source].statements.len(),
        };
        let destination_loc = mir::Location {
            block: destination,
            statement_index: 0,
        };
        let stmts = self.encode_expiring_borrows_between(source_loc, destination_loc)?;

        if force_block || !stmts.is_empty() {
            let edge_label = self.cfg_method.get_fresh_label_name();
            let edge_block = self.cfg_method.add_block(
                &edge_label,
                vec![],
                vec![
                    vir::Stmt::comment(format!("========== {} ==========", edge_label)),
                    vir::Stmt::comment(format!("MIR edge {:?} --> {:?}", source, destination)),
                ],
            );
            if !stmts.is_empty() {
                self.cfg_method
                    .add_stmt(edge_block, vir::Stmt::comment("Expire borrows"));
                self.cfg_method.add_stmts(edge_block, stmts);
            }
            Ok(Some(edge_block))
        } else {
            Ok(None)
        }
    }

    fn encode_impure_function_call(
        &mut self,
        location: mir::Location,
        call_site_span: rustc_span::Span,
        args: &[mir::Operand<'tcx>],
        destination: &Option<(mir::Place<'tcx>, BasicBlockIndex)>,
        called_def_id: ProcedureDefId,
        self_ty: Option<&'tcx ty::TyS<'tcx>>,
    ) -> Result<Vec<vir::Stmt>> {
        let full_func_proc_name = &self
            .encoder
            .env()
            .tcx()
            .def_path_str(called_def_id);
            // .absolute_item_path_str(called_def_id);
        debug!("Encoding non-pure function call '{}'", full_func_proc_name);

        let mut stmts = vec![];
        let mut stmts_after: Vec<vir::Stmt> = vec![];

        // Arguments can be places or constants. For constants, we pretend they're places by
        // creating a new local variable of the same type. For arguments that are not just local
        // variables (i.e., for places that have projections), we do the same. We don't replace
        // arguments that are just local variables with a new local variable.
        // This data structure maps the newly created local variables to the expression that was
        // originally passed as an argument.
        let mut fake_exprs: HashMap<vir::Expr, vir::Expr> = HashMap::new();
        let mut arguments = vec![];

        let mut const_arg_vars: HashSet<vir::Expr> = HashSet::new();
        let mut type_invs: HashMap<String, vir::Function> = HashMap::new();
        let mut constant_args = Vec::new();
        let mut arg_tys = Vec::new();

        for operand in args.iter() {
            let arg_ty = self.mir_encoder.get_operand_ty(operand);
            arg_tys.push(arg_ty);

            let arg = match operand {
                mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                    if let Some(local) = place.as_local() {
                        local.into()
                    } else {
                        self.locals.get_fresh(arg_ty)
                    }
                }
                mir::Operand::Constant(_) =>
                    self.locals.get_fresh(arg_ty)
            };
            arguments.push(arg.clone());

            let encoded_local = self.encode_prusti_local(arg);
            let arg_place = vir::Expr::local(encoded_local);
            debug!("arg: {:?} {}", arg, arg_place);
            let inv_name = self.encoder.encode_type_invariant_use(arg_ty);
            let arg_inv = self.encoder.encode_type_invariant_def(arg_ty);
            type_invs.insert(inv_name, arg_inv);
            match self.mir_encoder.encode_operand_place(operand) {
                Some(place) => {
                    debug!("arg: {} {}", arg_place, place);
                    fake_exprs.insert(arg_place, place.into());
                }
                None => {
                    // We have a constant.
                    constant_args.push(arg_place.clone());
                    let arg_val_expr = self.mir_encoder.encode_operand_expr(operand);
                    debug!("arg_val_expr: {} {}", arg_place, arg_val_expr);
                    let val_field = self.encoder.encode_value_field(arg_ty);
                    fake_exprs.insert(arg_place.clone().field(val_field), arg_val_expr);
                    let in_loop = self.loop_encoder.get_loop_depth(location.block) > 0;
                    if in_loop {
                        const_arg_vars.insert(arg_place);
                        return Err(EncodingError::unsupported(
                            format!(
                                "please use a local variable as argument for function '{}', not a \
                                constant, when calling the function from a loop",
                                full_func_proc_name
                            ),
                            call_site_span,
                        ));
                    }
                }
            }
        }

        let (target_local, encoded_target) = {
            match destination.as_ref() {
                Some((ref target_place, _)) => {
                    // will panic if attempting to encode unsupported type
                    let (encoded_target, ty, _) = self.mir_encoder.encode_place(target_place).unwrap();
                    let target_local = if let Some(target_local) = target_place.as_local() {
                        target_local.into()
                    } else {
                        self.locals.get_fresh(ty)
                    };
                    fake_exprs.insert(
                        vir::Expr::local(self.encode_prusti_local(target_local)),
                        encoded_target.clone().into(),
                    );
                    (target_local, Some(encoded_target))
                }
                None => {
                    // The return type is Never
                    // This means that the function call never returns
                    // So, we `assume false` after the function call
                    stmts_after.push(vir::Stmt::Inhale(false.into(), vir::FoldingBehaviour::Stmt));
                    // Return a dummy local variable
                    let never_ty = self.encoder.env().tcx().mk_ty(ty::TyKind::Never);
                    (self.locals.get_fresh(never_ty), None)
                }
            }
        };

        let replace_fake_exprs = |mut expr: vir::Expr| -> vir::Expr {
            for (fake_arg, arg_expr) in fake_exprs.iter() {
                expr = expr
                    .fold_expr(|orig_expr| {
                        // Inline or skip usages of constant parameters
                        // See issue #85
                        match orig_expr {
                            vir::Expr::FuncApp(ref name, ref args, _, _, _) => {
                                if args.len() == 1
                                    && args[0].is_local()
                                    && const_arg_vars.contains(&args[0])
                                {
                                    // Inline type invariant
                                    type_invs[name].inline_body(args.clone())
                                } else {
                                    orig_expr
                                }
                            }
                            vir::Expr::PredicateAccessPredicate(_, ref arg, _, _) => {
                                if arg.is_local() && const_arg_vars.contains(arg) {
                                    // Skip predicate permission
                                    true.into()
                                } else {
                                    orig_expr
                                }
                            }

                            x => x,
                        }
                    })
                    .replace_place(&fake_arg, arg_expr);
            }
            expr
        };

        let procedure_contract = {
            self.encoder.get_procedure_contract_for_call(
                self_ty,
                called_def_id,
                &arguments,
                target_local,
            )
        };

        // Store a label for the pre state
        let pre_label = self.cfg_method.get_fresh_label_name();
        stmts.push(vir::Stmt::Label(pre_label.clone()));

        // Havoc and inhale variables that store constants
        for constant_arg in &constant_args {
            stmts.extend(self.encode_havoc_and_allocation(constant_arg));
        }

        // Encode precondition.
        let (
            pre_type_spec,
            pre_mandatory_type_spec,
            pre_invs_spec,
            pre_func_spec,
            _, // We don't care about verifying that the weakening is valid,
               // since it isn't the task of the caller
        ) = self.encode_precondition_expr(&procedure_contract, None);
        let pos = self
            .encoder
            .error_manager()
            .register(call_site_span, ErrorCtxt::ExhaleMethodPrecondition);
        stmts.push(vir::Stmt::Assert(
            replace_fake_exprs(pre_func_spec),
            vir::FoldingBehaviour::Stmt, // TODO: Should be Expr.
            pos,
        ));
        stmts.push(vir::Stmt::Assert(
            replace_fake_exprs(pre_invs_spec),
            vir::FoldingBehaviour::Stmt,
            pos,
        ));
        let pre_perm_spec = replace_fake_exprs(pre_type_spec.clone());
        assert!(!pos.is_default());
        stmts.push(vir::Stmt::Exhale(
            pre_perm_spec.remove_read_permissions(),
            pos,
        ));

        // Move all read permissions that are taken by magic wands into pre
        // state and exhale only before the magic wands are inhaled. In this
        // way we can have specifications that link shared reference arguments
        // and shared reference result.
        let pre_mandatory_perms: Vec<_> = pre_mandatory_type_spec
            .into_iter()
            .map(&replace_fake_exprs)
            .collect();
        let mut pre_mandatory_perms_old = Vec::new();
        for perm in pre_mandatory_perms {
            let from_place = perm.get_place().unwrap().clone();
            let to_place = from_place.clone().old(pre_label.clone());
            let old_perm = perm.replace_place(&from_place, &to_place);
            stmts.push(vir::Stmt::TransferPerm(from_place, to_place, true));
            pre_mandatory_perms_old.push(old_perm);
        }
        let pre_mandatory_perm_spec = pre_mandatory_perms_old.into_iter().conjoin();

        // Havoc the content of the lhs, if there is one
        if let Some(ref target_place) = encoded_target {
            stmts.extend(self.encode_havoc(target_place));
        }

        // Store a label for permissions got back from the call
        debug!(
            "Procedure call location {:?} has label {}",
            location, pre_label
        );
        self.label_after_location
            .insert(location, pre_label.clone());

        // Store a label for the post state
        let post_label = self.cfg_method.get_fresh_label_name();

        self.call_labels.insert(location, (pre_label.clone(), post_label.clone()));

        // TODO: let loan = self.polonius_info().get_call_loan_at_location(location);
        let loan = None;
        let (
            post_type_spec,
            return_type_spec,
            post_invs_spec,
            post_func_spec,
            magic_wands,
            read_transfer,
            _, // We don't care about verifying that the strengthening is valid,
               // since it isn't the task of the caller
        ) = self.encode_postcondition_expr(
            Some(location),
            &procedure_contract,
            None,
            &pre_label,
            &post_label,
            Some((location, &fake_exprs)),
            encoded_target.is_none(),
            loan,
            false,
        )?;

        // We inhale the magic wand just before applying it because we need
        // a magic wand that depends on the current value of ghost variables.
        self.replace_old_places_with_ghost_vars(Some(&post_label), magic_wands);

        let post_perm_spec = replace_fake_exprs(post_type_spec);
        stmts.push(vir::Stmt::Inhale(
            post_perm_spec.remove_read_permissions(),
            vir::FoldingBehaviour::Stmt,
        ));
        if let Some(access) = return_type_spec {
            stmts.push(vir::Stmt::Inhale(
                replace_fake_exprs(access),
                vir::FoldingBehaviour::Stmt,
            ));
        }
        for (from_place, to_place) in read_transfer {
            stmts.push(vir::Stmt::TransferPerm(
                replace_fake_exprs(from_place),
                replace_fake_exprs(to_place),
                true,
            ));
        }
        stmts.push(vir::Stmt::Inhale(
            replace_fake_exprs(post_invs_spec),
            vir::FoldingBehaviour::Stmt,
        ));
        stmts.push(vir::Stmt::Inhale(
            replace_fake_exprs(post_func_spec),
            vir::FoldingBehaviour::Expr,
        ));

        // Exhale the permissions that were moved into magic wands.
        assert!(!pos.is_default());
        stmts.push(vir::Stmt::Exhale(pre_mandatory_perm_spec, pos));

        // Emit the label and magic wands
        stmts.push(vir::Stmt::Label(post_label.clone()));

        stmts.extend(stmts_after);

        self.procedure_contracts
            .insert(location, (procedure_contract, fake_exprs));

        Ok(stmts)
    }

    fn encode_pure_function_call(
        &mut self,
        location: mir::Location,
        call_site_span: rustc_span::Span,
        args: &[mir::Operand<'tcx>],
        destination: &Option<(mir::Place<'tcx>, BasicBlockIndex)>,
        called_def_id: ProcedureDefId,
    ) -> Vec<vir::Stmt> {
        let (function_name, return_type) = self.encoder.encode_pure_function_use(called_def_id);
        debug!("Encoding pure function call '{}'", function_name);
        assert!(destination.is_some());

        let mut arg_exprs = vec![];
        for operand in args.iter() {
            let arg_expr = self.mir_encoder.encode_operand_expr(operand);
            arg_exprs.push(arg_expr);
        }

        self.encode_specified_pure_function_call(
            location,
            call_site_span,
            args,
            destination,
            function_name,
            arg_exprs,
            return_type,
        )
    }

    fn encode_specified_pure_function_call(
        &mut self,
        location: mir::Location,
        call_site_span: Span,
        args: &[mir::Operand<'tcx>],
        destination: &Option<(mir::Place<'tcx>, BasicBlockIndex)>,
        function_name: String,
        arg_exprs: Vec<Expr>,
        return_type: Type,
    ) -> Vec<vir::Stmt> {
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
            .register(call_site_span, ErrorCtxt::PureFunctionCall);

        let func_call = vir::Expr::func_app(
            function_name,
            arg_exprs,
            formal_args,
            return_type.clone(),
            pos
        );

        let target_value = self.encode_pure_function_call_lhs_value(destination);

        let inhaled_expr = if return_type.is_domain() {
            let predicate_name = target_value.get_type().name();
            let snapshot = self.encoder.encode_snapshot_use(predicate_name);
            let target_place = self.encode_pure_function_call_lhs_place(destination);
            let snap_call = snapshot.get_snap_call(target_place);
            vir::Expr::eq_cmp(snap_call.clone(), func_call)
        } else {
            vir::Expr::eq_cmp(target_value.into(), func_call)
        };

        let (mut stmts,label) = self.encode_pure_function_call_site(
            location,
            destination,
            inhaled_expr
        );

        self.encode_transfer_args_permissions(location, args,  &mut stmts, label);
        stmts
    }

    fn encode_pure_function_call_lhs_value(
        &mut self,
        destination: &Option<(mir::Place<'tcx>, BasicBlockIndex)>,
    ) -> vir::Expr {
        match destination.as_ref() {
            Some((ref dst, _)) => self.mir_encoder.eval_place(dst),
            None => unreachable!(),
        }
    }

    fn encode_pure_function_call_lhs_place(
        &mut self,
        destination: &Option<(mir::Place<'tcx>, BasicBlockIndex)>,
    ) -> vir::Expr {
        match destination.as_ref() {
            // will panic if attempting to encode unsupported type
            Some((ref dst, _)) => self.mir_encoder.encode_place(dst).unwrap().0,
            None => unreachable!(),
        }
    }

    fn encode_pure_function_call_site(
        &mut self,
        location: mir::Location,
        destination: &Option<(mir::Place<'tcx>, BasicBlockIndex)>,
        call_result: vir::Expr,
    ) -> (Vec<vir::Stmt>,String) {
        let mut stmts = vec![];

        let label = self.cfg_method.get_fresh_label_name();
        stmts.push(vir::Stmt::Label(label.clone()));

        // Havoc the content of the lhs
        let target_place = self.encode_pure_function_call_lhs_place(destination);
        stmts.extend(self.encode_havoc(&target_place));
        let type_predicate = self
            .mir_encoder
            .encode_place_predicate_permission(target_place.clone(), vir::PermAmount::Write)
            .unwrap();

        stmts.push(vir::Stmt::Inhale(
            type_predicate,
            vir::FoldingBehaviour::Stmt,
        ));

        // Initialize the lhs
        stmts.push(
            vir::Stmt::Inhale(
                call_result,
                vir::FoldingBehaviour::Stmt,
            )
        );

        // Store a label for permissions got back from the call
        debug!(
            "Pure function call location {:?} has label {}",
            location, label
        );
        self.label_after_location.insert(location, label.clone());

        (stmts, label)
    }

    // Transfer the permissions for the arguments used in the call
    fn encode_transfer_args_permissions(
        &mut self,
        location: mir::Location,
        args: &[mir::Operand<'tcx>],
        stmts: &mut Vec<vir::Stmt>,
        label: String,
    )  {
        for operand in args.iter() {
            let operand_ty = self.mir_encoder.get_operand_ty(operand);
            let operand_place = self.mir_encoder.encode_operand_place(operand);
            match (operand_place, &operand_ty.kind()) {
                (
                    Some(ref place),
                    ty::TyKind::RawPtr(ty::TypeAndMut {
                        ty: ref inner_ty, ..
                    }),
                )
                | (Some(ref place), ty::TyKind::Ref(_, ref inner_ty, _)) => {
                    let ref_field = self.encoder.encode_dereference_field(inner_ty);
                    let ref_place = place.clone().field(ref_field);
                    stmts.extend(self.encode_transfer_permissions(
                        ref_place.clone(),
                        ref_place.clone().old(&label),
                        location,
                    ));
                }
                _ => {} // Nothing
            }
        }

        /*
        // Hack to work around the missing loan for arguments moved to the function call
        for operand in args.iter() {
            if let Some(place) = self.mir_encoder.encode_operand_place(operand) {
                debug!("Put permission {:?} in postcondition", place);
                // Choose the label that corresponds to the creation of the loan
                let (loans, _) = self.polonius_info().get_all_active_loans(location);
                let source_loans: Vec<_> = loans.iter().filter(|loan| {
                    let loan_places = self.polonius_info().get_loan_places(loan).unwrap();
                    let (expiring, _, restored) = self.encode_loan_places(&loan_places);
                    trace!("Try {:?} == {:?} | {:?}", expiring, place, restored);
                    expiring.parent() == Some(&place)
                }).collect();
                if !source_loans.is_empty() {
                    assert_eq!(source_loans.len(), 1, "The argument depends on a condition");
                    let source_loan = &source_loans[0];
                    let loan_loc = self.polonius_info().get_loan_location(&source_loan);
                    let loan_label = &self.label_after_location[&loan_loc];
                    stmts.push(vir::Stmt::TransferPerm(
                        place.clone(),
                        place.clone().old(&loan_label)
                    ));
                }
            }
        }
        */
    }

    /// Encode permissions that are implicitly carried by the given local variable.
    fn encode_local_variable_permission(&self, local: Local) -> vir::Expr {
        match self.locals.get_type(local).kind() {
            ty::TyKind::RawPtr(ty::TypeAndMut {
                ref ty,
                mutbl: mutability,
            })
            | ty::TyKind::Ref(_, ref ty, mutability) => {
                // Use unfolded references.
                let encoded_local = self.encode_prusti_local(local);
                let field = self.encoder.encode_dereference_field(ty);
                let place = vir::Expr::from(encoded_local).field(field);
                let perm_amount = match mutability {
                    Mutability::Mut => vir::PermAmount::Write,
                    Mutability::Not => vir::PermAmount::Read,
                };
                vir::Expr::and(
                    vir::Expr::acc_permission(place.clone(), vir::PermAmount::Write),
                    vir::Expr::pred_permission(place, perm_amount).unwrap(),
                )
            }
            _ => self
                .mir_encoder
                .encode_place_predicate_permission(
                    self.encode_prusti_local(local).into(),
                    vir::PermAmount::Write,
                )
                .unwrap(),
        }
    }

    /// Encode the precondition with three expressions:
    /// - one for the type encoding
    /// - one for the type invariants
    /// - one for the functional specification.
    fn encode_precondition_expr(
        &self,
        contract: &ProcedureContract<'tcx>,
        precondition_weakening: Option<typed::Assertion<'tcx>>,
    ) -> (
        vir::Expr,
        Vec<vir::Expr>,
        vir::Expr,
        vir::Expr,
        Option<vir::Expr>,
    ) {
        // Type spec in which read permissions can be removed.
        let mut type_spec = Vec::new();
        // Type spec containing the read permissions that must be exhaled because they were
        // moved into a magic wand.
        let mut mandatory_type_spec = Vec::new();
        let is_blocked = |arg: Local|
            contract.borrow_infos.blocked.iter().any(|p| p.is_root(arg));
        for local in &contract.args {
            let mut add = |access: vir::Expr| {
                if is_blocked(*local)
                    && access.get_perm_amount() == vir::PermAmount::Read
                {
                    mandatory_type_spec.push(access);
                } else {
                    type_spec.push(access);
                }
            };
            let access = self.encode_local_variable_permission(*local);
            match access {
                vir::Expr::BinOp(vir::BinOpKind::And, box access1, box access2, _) => {
                    add(access1);
                    add(access2);
                }
                _ => add(access),
            };
        }

        let mut invs_spec: Vec<vir::Expr> = vec![];

        for arg in contract.args.iter() {
            invs_spec.push(self.encoder.encode_invariant_func_app(
                self.locals.get_type(*arg),
                self.encode_prusti_local(*arg).into(),
            ));
        }

        let mut func_spec: Vec<vir::Expr> = vec![];

        // Encode functional specification
        let encoded_args: Vec<vir::Expr> = contract
            .args
            .iter()
            .map(|local| self.encode_prusti_local(*local).into())
            .collect();
        let func_precondition = contract.functional_precondition();
        for assertion in func_precondition {
            // FIXME
            let value = self.encoder.encode_assertion(
                &assertion,
                &self.mir,
                None,
                &encoded_args,
                None,
                false,
                None,
                ErrorCtxt::GenericExpression,
            );
            func_spec.push(value);
        }
        let precondition_weakening = precondition_weakening.map(|pw| {
            self.encoder.encode_assertion(
                &pw,
                &self.mir,
                None,
                &encoded_args,
                None,
                false,
                None,
                ErrorCtxt::AssertMethodPreconditionWeakening(MultiSpan::from_spans(
                    func_precondition
                        .iter()
                        .flat_map(|ts| typed::Spanned::get_spans(ts, &self.mir, self.encoder.env().tcx()))
                        .collect(),
                )),
            )
        });
        (
            type_spec.into_iter().conjoin(),
            mandatory_type_spec,
            invs_spec.into_iter().conjoin(),
            func_spec
                .into_iter()
                .map(|spec| SnapshotSpecPatcher::new(self.encoder).patch_spec(spec))
                .conjoin(),
            precondition_weakening,
        )
    }

    /// Encode precondition inhale on the definition side.
    fn encode_preconditions(
        &mut self,
        start_cfg_block: CfgBlockIndex,
        precondition_weakening: Option<typed::Assertion<'tcx>>,
    ) {
        self.cfg_method
            .add_stmt(start_cfg_block, vir::Stmt::comment("Preconditions:"));
        let (type_spec, mandatory_type_spec, invs_spec, func_spec, weakening_spec) =
            self.encode_precondition_expr(self.procedure_contract(), precondition_weakening);
        self.cfg_method.add_stmt(
            start_cfg_block,
            vir::Stmt::Inhale(type_spec, vir::FoldingBehaviour::Stmt),
        );
        self.cfg_method.add_stmt(
            start_cfg_block,
            vir::Stmt::Inhale(
                mandatory_type_spec.into_iter().conjoin(),
                vir::FoldingBehaviour::Stmt,
            ),
        );
        self.cfg_method.add_stmt(
            start_cfg_block,
            vir::Stmt::Inhale(invs_spec, vir::FoldingBehaviour::Stmt),
        );
        // Weakening assertion must be put before inhaling the precondition, otherwise the weakening
        // soundness check becomes trivially satisfied.
        if let Some(weakening_spec) = weakening_spec {
            let pos = weakening_spec.pos();
            self.cfg_method.add_stmt(
                start_cfg_block,
                vir::Stmt::Assert(weakening_spec, FoldingBehaviour::Expr, pos),
            );
        }
        self.cfg_method.add_stmt(
            start_cfg_block,
            vir::Stmt::Inhale(func_spec, vir::FoldingBehaviour::Expr),
        );
        self.cfg_method.add_stmt(
            start_cfg_block,
            vir::Stmt::Label(PRECONDITION_LABEL.to_string()),
        );
    }

    /// Encode the magic wand used in the postcondition with its
    /// functional specification. Returns (lhs, rhs).
    ///
    /// * `location` is the location of the function call (if we're encoding a function call),
    /// otherwise (if we encode the function itself) it is `None`.
    fn encode_postcondition_expiration_tool(
        &mut self,
        location: Option<mir::Location>,
        contract: &ProcedureContract<'tcx>,
        pre_label: &str, post_label: &str
    ) -> Result<Option<vir::Expr>> {
        let borrow_infos = &contract.borrow_infos;
        if borrow_infos.blocked.is_empty() {
            return Ok(None)
        }

        let pledges = match &contract.specification {
            SpecificationSet::Procedure(specification) => &specification.pledges,
            _ => unreachable!(),
        };

        let pledges = pledges.iter()
            .map(|pledge| pledge.rhs.clone())
            .collect();

        let def_id = ty::WithOptConstParam::unknown(contract.def_id.expect_local());
        let tcx = self.procedure.get_tcx();
        let (mir, _) = tcx.mir_promoted(def_id);

        let mut carrier = ExpirationToolCarrier::default();
        let expiration_tool = carrier.construct(tcx, &mir.borrow(), borrow_infos, pledges)?;
        let expiration_tool = self.encode_expiration_tool_as_expression(
            expiration_tool, contract, location, pre_label, post_label);
        Ok(Some(expiration_tool))
    }

    /// Wrap function arguments used in the postcondition into ``old``:
    ///
    /// +   For references wrap the base ``_1.var_ref``.
    /// +   For non-references wrap the entire place into old.
    pub fn wrap_arguments_into_old(
        &self,
        mut assertion: vir::Expr,
        pre_label: &str,
        contract: &ProcedureContract<'tcx>,
        encoded_args: &[vir::Expr],
    ) -> vir::Expr {
        for (encoded_arg, &arg) in encoded_args.iter().zip(&contract.args) {
            let ty = self.locals.get_type(arg);
            if self.mir_encoder.is_reference(ty) {
                // If the argument is a reference, we wrap _1.val_ref into old.
                let (encoded_deref, ..) = self.mir_encoder.encode_deref(encoded_arg.clone(), ty);
                let original_expr = encoded_deref;
                let old_expr = vir::Expr::labelled_old(pre_label, original_expr.clone());
                assertion = assertion.replace_place(&original_expr, &old_expr);
            } else {
                // If the argument is not a reference, we wrap entire path into old.
                assertion = assertion.fold_places(|place| {
                    let base: vir::Expr = place.get_base().into();
                    if encoded_arg == &base {
                        place.old(pre_label)
                    } else {
                        place
                    }
                });
            }
        }
        assertion.remove_redundant_old()
    }

    /// Encode the postcondition with three expressions:
    /// - one for the type encoding
    /// - one for the type invariants
    /// - one for the functional specification.
    /// Also return the magic wands to be added to the postcondition.
    ///
    /// `function_end` – are we encoding the exhale of the postcondition
    /// at the end of the method?
    fn encode_postcondition_expr(
        &mut self,
        location: Option<mir::Location>,
        contract: &ProcedureContract<'tcx>,
        postcondition_strengthening: Option<typed::Assertion<'tcx>>,
        pre_label: &str,
        post_label: &str,
        magic_wand_store_info: Option<(mir::Location, &HashMap<vir::Expr, vir::Expr>)>,
        _diverging: bool,
        loan: Option<facts::Loan>,
        function_end: bool,
    ) -> Result<(
        vir::Expr,                   // Returned permissions from types.
        Option<vir::Expr>,           // Permission of the return value.
        vir::Expr,                   // Invariants.
        vir::Expr,                   // Functional specification.
        vir::Expr,                   // Magic wands.
        Vec<(vir::Expr, vir::Expr)>, // Read permissions that need to be transferred to a new place.
        Option<vir::Expr>, // Specification strengthening, in case of trait method implementation.
    )> {
        let mut type_spec = vec![];
        let mut invs_spec = vec![];
        let mut read_transfer = vec![]; // Permissions taken as read
                                        // references that need to
                                        // be transfered to old.

        // Encode the permissions got back and invariants for the arguments of type reference
        for (place, mutability) in contract.returned_refs.iter() {
            debug!(
                "Put permission {:?} ({:?}) in postcondition",
                place, mutability
            );
            let (place_expr, place_ty, _) = self.encode_generic_place(
                contract.def_id, location, place);
            let old_place_expr = place_expr.clone().old(pre_label);
            let mut add_type_spec = |perm_amount| {
                let permissions =
                    vir::Expr::pred_permission(old_place_expr.clone(), perm_amount).unwrap();
                type_spec.push(permissions);
            };
            match mutability {
                Mutability::Not => {
                    if function_end {
                        add_type_spec(vir::PermAmount::Read);
                    }
                    read_transfer.push((place_expr, old_place_expr));
                }
                Mutability::Mut => {
                    add_type_spec(vir::PermAmount::Write);
                    let inv = self
                        .encoder
                        .encode_invariant_func_app(place_ty, old_place_expr);
                    invs_spec.push(inv);
                }
            };
        }

        // Encode args and return.
        let encoded_args: Vec<vir::Expr> = contract
            .args
            .iter()
            .map(|local| self.encode_prusti_local(*local).into())
            .collect();
        trace!("encode_postcondition_expr: encoded_args {:?} ({:?}) as {:?}", contract.args,
               contract.args.iter().map(|a| self.locals.get_type(*a)).collect::<Vec<_>>(),
               encoded_args);

        let encoded_return: vir::Expr = self.encode_prusti_local(contract.returned_value).into();

        let magic_wands = self.encode_postcondition_expiration_tool(
            location, contract, pre_label, post_label)?;
        let magic_wands = if let Some(mut magic_wands) = magic_wands {
            if let Some((location, fake_exprs)) = magic_wand_store_info {
                for (fake_arg, arg_expr) in fake_exprs.iter() {
                    magic_wands = magic_wands.replace_place(&fake_arg, arg_expr);
                }
                // debug!("Insert ({:?} {:?}) at {:?}", lhs, rhs, location);
                // self.magic_wand_at_location
                //     .insert(location, (post_label.to_string(), lhs.clone(), rhs.clone()));
            }
            magic_wands
        } else {
            vir::Expr::Const(vir::Const::Bool(true), Position::default())
        };

        // Encode permissions for return type
        // TODO: Clean-up: remove unnecessary Option.
        let return_perm = Some(self.encode_local_variable_permission(contract.returned_value));

        // Encode invariant for return value
        // TODO put this in the above if?
        invs_spec.push(self.encoder.encode_invariant_func_app(
            self.locals.get_type(contract.returned_value),
            encoded_return.clone(),
        ));

        // Encode functional specification
        let mut func_spec = vec![];
        let mut func_spec_spans = vec![];
        let func_postcondition = contract.functional_postcondition();
        for typed_assertion in func_postcondition {
            let mut assertion = self.encoder.encode_assertion(
                &typed_assertion,
                &self.mir,
                Some(pre_label),
                &encoded_args,
                Some(&encoded_return),
                false,
                None,
                ErrorCtxt::GenericExpression,
            );
            func_spec_spans.extend(typed::Spanned::get_spans(typed_assertion, &self.mir, self.encoder.env().tcx()));
            assertion = self.wrap_arguments_into_old(assertion, pre_label, contract, &encoded_args);
            func_spec.push(assertion);
        }
        let func_spec_pos = self.encoder.error_manager().register_span(func_spec_spans);

        // Encode possible strengthening, in case of trait method implementation
        let strengthening_spec = postcondition_strengthening.map(|ps| {
            let assertion = self.encoder.encode_assertion(
                &ps,
                &self.mir,
                Some(pre_label),
                &encoded_args,
                Some(&encoded_return),
                false,
                None,
                ErrorCtxt::AssertMethodPostconditionStrengthening(MultiSpan::from_spans(
                    func_postcondition
                        .iter()
                        .flat_map(|ts| typed::Spanned::get_spans(ts, &self.mir, self.encoder.env().tcx()))
                        .collect(),
                )),
            );
            self.wrap_arguments_into_old(assertion, pre_label, contract, &encoded_args)
        });

        let full_func_spec = func_spec
            .into_iter()
            .map( // patch type mismatches for specs involving pure functions returning copy types
                |spec| SnapshotSpecPatcher::new(self.encoder).patch_spec(spec)
            ).conjoin()
            .set_default_pos(func_spec_pos);

        Ok((
            type_spec.into_iter().conjoin(),
            return_perm,
            invs_spec.into_iter().conjoin(),
            full_func_spec,
            magic_wands,
            read_transfer,
            strengthening_spec,
        ))
    }

    /// Modelling move as simple assignment on Viper level has a consequence
    /// that the assigned place changes. Therefore, if some value is
    /// moved into a borrow, the borrow starts pointing to a different
    /// memory location. As a result, we cannot use old expressions as
    /// roots for holding permissions because they always point to the
    /// same place. Instead, we replace them with ghost variables.
    ///
    /// This method replaces all places with `label` with ghost variables.
    fn replace_old_places_with_ghost_vars(
        &mut self,
        label: Option<&str>,
        expr: vir::Expr,
    ) -> vir::Expr {
        struct OldReplacer<'a> {
            label: Option<&'a str>,
            old_to_ghost_var: &'a mut HashMap<vir::Expr, vir::Expr>,
            old_ghost_vars: &'a mut HashMap<String, vir::Type>,
            cfg_method: &'a mut vir::CfgMethod,
        }
        impl<'a> vir::ExprFolder for OldReplacer<'a> {
            fn fold_labelled_old(
                &mut self,
                label: String,
                base: Box<vir::Expr>,
                pos: vir::Position,
            ) -> vir::Expr {
                let base = self.fold_boxed(base);
                let expr = vir::Expr::LabelledOld(label.clone(), base, pos);
                debug!(
                    "replace_old_places_with_ghost_vars({:?}, {})",
                    self.label, expr
                );
                if self.old_to_ghost_var.contains_key(&expr) {
                    debug!("found={}", self.old_to_ghost_var[&expr]);
                    self.old_to_ghost_var[&expr].clone().set_pos(pos)
                } else if self.label == Some(&label) {
                    let mut counter = 0;
                    let mut name = format!("_old${}${}", label, counter);
                    while self.old_ghost_vars.contains_key(&name) {
                        counter += 1;
                        name = format!("_old${}${}", label, counter);
                    }
                    let vir_type = expr.get_type().clone();
                    self.old_ghost_vars.insert(name.clone(), vir_type.clone());
                    self.cfg_method.add_local_var(&name, vir_type.clone());
                    let var: vir::Expr = vir::LocalVar::new(name, vir_type).into();
                    self.old_to_ghost_var.insert(expr, var.clone());
                    var
                } else {
                    debug!("not found");
                    expr
                }
            }
        }
        let mut replacer = OldReplacer {
            label: label,
            old_to_ghost_var: &mut self.old_to_ghost_var,
            old_ghost_vars: &mut self.old_ghost_vars,
            cfg_method: &mut self.cfg_method,
        };
        vir::ExprFolder::fold(&mut replacer, expr)
    }

    /// Encode the package statement of magic wands at the end of the method
    fn encode_package_end_of_method(
        &mut self,
        pre_label: &str,
        post_label: &str,
        location: mir::Location,
    ) -> Result<Vec<vir::Stmt>> {
        // TODO: We clone here because `self` will be borrowed mutably later.
        let contract = self.procedure_contract.clone().unwrap();

        let pledges = contract.pledges().iter()
            .map(|pledge| pledge.rhs.clone())
            .collect();

        let reborrow_signature = &contract.borrow_infos;
        let tcx = self.procedure.get_tcx();
        let mut carrier = ExpirationToolCarrier::default();
        let expiration_tool = carrier.construct(tcx, self.mir, reborrow_signature, pledges)?;

        self.encode_expiration_tool_as_package(
            &expiration_tool, &contract, location, pre_label, post_label)
    }

    /// Encode postcondition exhale in the `return_cfg_block` CFG block.
    fn encode_postconditions(
        &mut self,
        return_cfg_block: CfgBlockIndex,
        postcondition_strengthening: Option<typed::Assertion<'tcx>>,
    ) -> Result<()> {
        // This clone is only due to borrow checker restrictions
        let contract = self.procedure_contract().clone();

        self.cfg_method
            .add_stmt(return_cfg_block, vir::Stmt::comment("Exhale postcondition"));

        let type_inv_pos = self.encoder.error_manager().register(
            self.mir.span,
            ErrorCtxt::AssertMethodPostconditionTypeInvariants,
        );

        let (type_spec, return_type_spec, invs_spec, func_spec, magic_wands, _, strengthening_spec) =
            self.encode_postcondition_expr(
                None,
                &contract,
                postcondition_strengthening,
                PRECONDITION_LABEL,
                POSTCONDITION_LABEL,
                None,
                false,
                None,
                true,
            )?;

        // Find which arguments are blocked by the returned reference.
        let blocked_places = contract.borrow_infos.blocked;
        let blocked_args = contract.args.iter().cloned().enumerate()
            .filter_map(|(i, arg)|
                if blocked_places.iter().any(|blocked| blocked.is_root(arg)) {
                    Some(i)
                } else {
                    None
                })
            .collect::<Vec<_>>();

        // Transfer borrow permissions to old.
        self.cfg_method.add_stmt(
            return_cfg_block,
            vir::Stmt::comment(
                "Fold predicates for &mut args and transfer borrow permissions to old",
            ),
        );
        for (i, &arg) in contract.args.iter().enumerate() {
            if blocked_args.contains(&i) {
                // Permissions of arguments that are blocked by the returned reference are not
                // added to the postcondition.
                continue;
            }
            let ty = self.locals.get_type(arg);
            if self.mir_encoder.is_reference(ty) {
                let encoded_arg: vir::Expr = self.encode_prusti_local(arg).into();
                let (encoded_deref, ..) = self.mir_encoder.encode_deref(encoded_arg.clone(), ty);

                // Fold argument.
                let deref_pred = self
                    .mir_encoder
                    .encode_place_predicate_permission(
                        encoded_deref.clone(),
                        vir::PermAmount::Write,
                    )
                    .unwrap();
                for stmt in self
                    .encode_obtain(deref_pred, type_inv_pos)
                    .drain(..)
                {
                    self.cfg_method.add_stmt(return_cfg_block, stmt);
                }

                // Transfer permissions.
                //
                // TODO: This version does not allow mutating function arguments.
                // A way to allow this would be for each reference typed
                // argument generate a fresh pure variable `v` and a
                // variable `b:=true` and add `old[pre](_1.val_ref)` to
                // the replacement map. Before each assignment that
                // assigns to the reference itself, emit `b:=false`.
                // After each assignment that assigns to the contents
                // the reference is pointing to emit:
                //
                //      if b {
                //          v := _1.val_ref;
                //      }
                let old_expr = encoded_deref.clone().old(PRECONDITION_LABEL);
                let name = format!("_old${}${}", PRECONDITION_LABEL, i);
                let vir_type = old_expr.get_type().clone();
                self.old_ghost_vars.insert(name.clone(), vir_type.clone());
                self.cfg_method.add_local_var(&name, vir_type.clone());
                let var: vir::Expr = vir::LocalVar::new(name, vir_type).into();
                self.old_to_ghost_var.insert(old_expr, var.clone());

                self.cfg_method.add_stmt(
                    return_cfg_block,
                    vir::Stmt::Assign(var, encoded_deref, vir::AssignKind::Move),
                );
            }
        }

        // Fold the result.
        self.cfg_method
            .add_stmt(return_cfg_block, vir::Stmt::comment("Fold the result"));
        let ty = self.locals.get_type(contract.returned_value);
        let encoded_return: vir::Expr = self.encode_prusti_local(contract.returned_value).into();
        let encoded_return_expr = if self.mir_encoder.is_reference(ty) {
            let (encoded_deref, ..) = self.mir_encoder.encode_deref(encoded_return, ty);
            encoded_deref
        } else {
            encoded_return
        };
        let return_pred = self
            .mir_encoder
            .encode_place_predicate_permission(encoded_return_expr.clone(), vir::PermAmount::Write)
            .unwrap();
        let obtain_return_stmt = vir::Stmt::Obtain(return_pred, type_inv_pos);
        self.cfg_method
            .add_stmt(return_cfg_block, obtain_return_stmt);

        // Assert possible strengthening
        if let Some(strengthening_spec) = strengthening_spec {
            let patched_strengthening_spec =
                self.replace_old_places_with_ghost_vars(None, strengthening_spec);
            let pos = patched_strengthening_spec.pos();
            self.cfg_method.add_stmt(
                return_cfg_block,
                vir::Stmt::Assert(patched_strengthening_spec, FoldingBehaviour::Expr, pos),
            );
        }
        // Assert functional specification of postcondition
        let func_pos = self
            .encoder
            .error_manager()
            .register(self.mir.span, ErrorCtxt::AssertMethodPostcondition);
        let patched_func_spec = self.replace_old_places_with_ghost_vars(None, func_spec);
        self.cfg_method.add_stmt(
            return_cfg_block,
            vir::Stmt::Assert(patched_func_spec, vir::FoldingBehaviour::Expr, func_pos),
        );

        // Assert type invariants
        let patched_invs_spec = self.replace_old_places_with_ghost_vars(None, invs_spec);
        self.cfg_method.add_stmt(
            return_cfg_block,
            vir::Stmt::Assert(patched_invs_spec, vir::FoldingBehaviour::Stmt, type_inv_pos),
        );

        // Exhale permissions of postcondition
        let perm_pos = self
            .encoder
            .error_manager()
            .register(self.mir.span, ErrorCtxt::ExhaleMethodPostcondition);
        let patched_type_spec = self.replace_old_places_with_ghost_vars(None, type_spec);
        assert!(!perm_pos.is_default());
        self.cfg_method.add_stmt(
            return_cfg_block,
            vir::Stmt::Exhale(patched_type_spec, perm_pos),
        );
        if let Some(access) = return_type_spec {
            self.cfg_method.add_stmt(
                return_cfg_block,
                vir::Stmt::Exhale(access, perm_pos),
            );
        }
        self.cfg_method.add_stmt(
            return_cfg_block,
            vir::Stmt::Exhale(magic_wands, perm_pos),
        );

        Ok(())
    }

    fn get_pure_var_for_preserving_value(
        &mut self,
        loop_head: BasicBlockIndex,
        place: &vir::Expr,
    ) -> vir::LocalVar {
        let loop_map = self
            .pure_var_for_preserving_value_map
            .get_mut(&loop_head)
            .unwrap();
        if let Some(local_var) = loop_map.get(place) {
            local_var.clone()
        } else {
            let mut counter = 0;
            let mut name = format!("_preserve${}", counter);
            while self.auxiliary_local_vars.contains_key(&name) {
                counter += 1;
                name = format!("_preserve${}", counter);
            }
            let vir_type = vir::Type::TypedRef(String::from("AuxRef"));
            self.cfg_method.add_local_var(&name, vir_type.clone());
            self.auxiliary_local_vars
                .insert(name.clone(), vir_type.clone());
            let var = vir::LocalVar::new(name, vir_type);
            loop_map.insert(place.clone(), var.clone());
            var
        }
    }

    /// Since the loop invariant is taking all permission from the
    /// outer context, we need to preserve values of references by
    /// saving them in local variables.
    fn construct_value_preserving_equality(
        &mut self,
        loop_head: BasicBlockIndex,
        place: &vir::Expr,
    ) -> vir::Expr {
        let tmp_var = self.get_pure_var_for_preserving_value(loop_head, place);
        vir::Expr::BinOp(
            vir::BinOpKind::EqCmp,
            box tmp_var.into(),
            box place.clone(),
            vir::Position::default(),
        )
    }

    /// Arguments:
    /// * `loop_head`: the loop head block, which identifies a loop.
    /// * `loop_inv`: the block at whose end the loop invariant should hold.
    /// * `drop_read_references`: should we add permissions to read
    ///   references? We drop permissions of read references from the
    ///   exhale before the loop and inhale after the loop so that
    ///   the knowledge about their values is not havocked.
    ///
    /// Result:
    /// * The first vector contains permissions.
    /// * The second vector contains value preserving equalities.
    fn encode_loop_invariant_permissions(
        &mut self,
        loop_head: BasicBlockIndex,
        loop_inv: BasicBlockIndex,
        drop_read_references: bool,
    ) -> (Vec<vir::Expr>, Vec<vir::Expr>) {
        trace!(
            "[enter] encode_loop_invariant_permissions \
             loop_head={:?} drop_read_references={}",
            loop_head,
            drop_read_references
        );
        let permissions_forest = self
            .loop_encoder
            .compute_loop_invariant(loop_head, loop_inv);
        debug!("permissions_forest: {:?}", permissions_forest);
        let loops = self.loop_encoder.get_enclosing_loop_heads(loop_head);
        let enclosing_permission_forest = if loops.len() > 1 {
            let next_to_last = loops.len() - 2;
            let enclosing_loop_head = loops[next_to_last];
            Some(self.loop_encoder.compute_loop_invariant(
                enclosing_loop_head,
                self.cached_loop_invariant_block[&enclosing_loop_head],
            ))
        } else {
            None
        };

        let mut permissions = Vec::new();
        let mut equalities = Vec::new();
        for tree in permissions_forest.get_trees().iter() {
            for (kind, mir_place) in tree.get_permissions().into_iter() {
                if kind.is_none() {
                    continue;
                }
                // will panic if attempting to encode unsupported type
                let (encoded_place, ty, _) = self.mir_encoder.encode_place(&mir_place).unwrap();
                debug!("kind={:?} mir_place={:?} ty={:?}", kind, mir_place, ty);
                if let ty::TyKind::Closure(..) = ty.kind() {
                    // Do not encode closures
                    continue;
                }
                match kind {
                    // Gives read permission to this node. It must not be a leaf node.
                    PermissionKind::ReadNode => {
                        let perm = vir::Expr::acc_permission(encoded_place, vir::PermAmount::Read);
                        permissions.push(perm);
                    }

                    // Gives write permission to this node. It must not be a leaf node.
                    PermissionKind::WriteNode => {
                        let perm = vir::Expr::acc_permission(encoded_place, vir::PermAmount::Write);
                        permissions.push(perm);
                    }

                    // Gives read or write permission to the entire
                    // subtree including this node. This must be a leaf
                    // node.
                    PermissionKind::ReadSubtree | PermissionKind::WriteSubtree => {
                        let perm_amount = match kind {
                            PermissionKind::WriteSubtree => vir::PermAmount::Write,
                            PermissionKind::ReadSubtree => vir::PermAmount::Read,
                            _ => unreachable!(),
                        };
                        let def_init = self
                            .loop_encoder
                            .is_definitely_initialised(&mir_place, loop_head);
                        debug!("    perm_amount={} def_init={}", perm_amount, def_init);
                        if let Some(base) = utils::try_pop_deref(self.encoder.env().tcx(), mir_place)
                        {
                            // will panic if attempting to encode unsupported type
                            let (_, ref_ty, _) = self.mir_encoder.encode_place(&base).unwrap();
                            match ref_ty.kind() {
                                ty::TyKind::RawPtr(ty::TypeAndMut { mutbl, .. })
                                | ty::TyKind::Ref(_, _, mutbl) => {
                                    if def_init {
                                        equalities.push(self.construct_value_preserving_equality(
                                            loop_head,
                                            &encoded_place,
                                        ));
                                    }
                                    if drop_read_references {
                                        if mutbl == &Mutability::Not {
                                            continue;
                                        }
                                    }
                                }
                                ref x => unreachable!("{:?}", x),
                            }
                        }
                        match ty.kind() {
                            ty::TyKind::RawPtr(ty::TypeAndMut { ref ty, mutbl })
                            | ty::TyKind::Ref(_, ref ty, mutbl) => {
                                debug!(
                                    "encode_loop_invariant_permissions \
                                     mir_place={:?} mutability={:?} \
                                     drop_read_references={}",
                                    mir_place, mutbl, drop_read_references
                                );
                                // Use unfolded references.
                                let field = self.encoder.encode_dereference_field(ty);
                                let field_place = vir::Expr::from(encoded_place).field(field);
                                permissions.push(vir::Expr::acc_permission(
                                    field_place.clone(),
                                    perm_amount,
                                ));
                                if def_init {
                                    equalities.push(self.construct_value_preserving_equality(
                                        loop_head,
                                        &field_place,
                                    ));
                                }
                                if def_init
                                    && !(mutbl == &Mutability::Not && drop_read_references)
                                {
                                    permissions.push(
                                        vir::Expr::pred_permission(field_place, perm_amount)
                                            .unwrap(),
                                    );
                                }
                            }
                            _ => {
                                permissions.push(
                                    vir::Expr::pred_permission(encoded_place, perm_amount).unwrap(),
                                );
                                if let Some(forest) = &enclosing_permission_forest {
                                    for child_place in forest.get_children(&mir_place) {
                                        // If the forest contains the place, but that place is a
                                        // regular node (either ReadNode or WriteNode), that means
                                        // that we will lose information about the children of that
                                        // place after the loop and we need to preserve it via local
                                        // variables.
                                        let (encoded_child, _, _) =
                                            self.mir_encoder.encode_place(&child_place).unwrap(); // will panic if attempting to encode unsupported type
                                        equalities.push(self.construct_value_preserving_equality(
                                            loop_head,
                                            &encoded_child,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    // This should be repalced with WriteNode and
                    // WriteSubtree before this point.
                    PermissionKind::WriteNodeAndSubtree => unreachable!(),
                    // Give no permission to this node and the entire subtree. This
                    // must be a leaf node.
                    PermissionKind::None => unreachable!(),
                };
            }
        }

        trace!(
            "[exit] encode_loop_invariant_permissions permissions={}",
            permissions
                .iter()
                .map(|p| format!("{}, ", p))
                .collect::<String>()
        );
        (permissions, equalities)
    }

    /// Get the basic blocks that encode the specification of a loop invariant
    fn get_loop_spec_blocks(&self, loop_head: BasicBlockIndex) -> Vec<BasicBlockIndex> {
        let mut res = vec![];
        for bbi in self.procedure.get_reachable_cfg_blocks() {
            if Some(loop_head) == self.loop_encoder.get_loop_head(bbi)
                && self.procedure.is_spec_block(bbi)
            {
                res.push(bbi)
            } else {
                debug!(
                    "bbi {:?} has head {:?} and 'is spec' is {}",
                    bbi,
                    self.loop_encoder.get_loop_head(bbi),
                    self.procedure.is_spec_block(bbi)
                );
            }
        }
        res
    }

    /// Encode the functional specification of a loop
    fn encode_loop_invariant_specs(
        &self,
        loop_head: BasicBlockIndex,
        loop_inv_block: BasicBlockIndex,
    ) -> (Vec<vir::Expr>, MultiSpan) {
        let spec_blocks = self.get_loop_spec_blocks(loop_head);
        trace!(
            "loop head {:?} has spec blocks {:?}",
            loop_head,
            spec_blocks
        );

        // `body_invariant!(..)` is desugared to a closure with special attributes,
        // which we can detect and use to retrieve the specification.
        let mut spec_ids = vec![];
        for bbi in spec_blocks {
            for stmt in &self.mir.basic_blocks()[bbi].statements {
                if let mir::StatementKind::Assign(box (
                    _,
                    mir::Rvalue::Aggregate(box mir::AggregateKind::Closure(cl_def_id, _), _),
                )) = stmt.kind {
                    spec_ids.extend(
                        self.encoder.get_loop_specs(cl_def_id)
                    );
                }
            }
        }
        trace!("spec_ids: {:?}", spec_ids);

        let mut encoded_specs = vec![];
        let mut encoded_spec_spans = vec![];
        if !spec_ids.is_empty() {
            let encoded_args: Vec<vir::Expr> = self
                .mir
                .args_iter()
                .map(|local| self.mir_encoder.encode_local(local).unwrap().into()) // will panic if attempting to encode unsupported type
                .collect();
            for spec_id in &spec_ids {
                let assertion = self.encoder.spec().get(spec_id).unwrap();
                // TODO: Mmm... are these parameters correct?
                let encoded_spec = self.encoder.encode_assertion(
                    &assertion,
                    &self.mir,
                    Some(PRECONDITION_LABEL),
                    &encoded_args,
                    None,
                    false,
                    Some(loop_inv_block),
                    ErrorCtxt::GenericExpression,
                );
                let spec_spans = typed::Spanned::get_spans(assertion, &self.mir, self.encoder.env().tcx());
                let spec_pos = self
                    .encoder
                    .error_manager()
                    .register_span(spec_spans.clone());
                encoded_specs.push(encoded_spec.set_default_pos(spec_pos));
                encoded_spec_spans.extend(spec_spans);
            }
            trace!("encoded_specs: {:?}", encoded_specs);
        }

        (encoded_specs, MultiSpan::from_spans(encoded_spec_spans))
    }

    fn encode_loop_invariant_exhale_stmts(
        &mut self,
        loop_head: BasicBlockIndex,
        loop_inv_block: BasicBlockIndex,
        after_loop_iteration: bool,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_loop_invariant_exhale_stmts loop_head={:?} \
             after_loop_iteration={}",
            loop_head,
            after_loop_iteration
        );
        if !after_loop_iteration {
            self.pure_var_for_preserving_value_map
                .insert(loop_head, HashMap::new());
        }
        let (permissions, equalities) =
            self.encode_loop_invariant_permissions(loop_head, loop_inv_block, true);
        let (func_spec, func_spec_span) =
            self.encode_loop_invariant_specs(loop_head, loop_inv_block);

        // TODO: use different positions, and generate different error messages, for the exhale
        // before the loop and after the loop body

        let assert_pos = self.encoder.error_manager().register(
            // TODO: choose a proper error span
            func_spec_span.clone(),
            if after_loop_iteration {
                ErrorCtxt::AssertLoopInvariantAfterIteration
            } else {
                ErrorCtxt::AssertLoopInvariantOnEntry
            },
        );

        let exhale_pos = self.encoder.error_manager().register(
            // TODO: choose a proper error span
            func_spec_span,
            if after_loop_iteration {
                ErrorCtxt::ExhaleLoopInvariantAfterIteration
            } else {
                ErrorCtxt::ExhaleLoopInvariantOnEntry
            },
        );

        let mut stmts = vec![vir::Stmt::comment(format!(
            "Assert and exhale the loop body invariant (loop head: {:?})",
            loop_head
        ))];
        if !after_loop_iteration {
            for (place, field) in &self.pure_var_for_preserving_value_map[&loop_head] {
                stmts.push(vir::Stmt::Assign(
                    field.into(),
                    place.clone(),
                    vir::AssignKind::Ghost,
                ));
            }
        }
        assert!(!assert_pos.is_default());
        let obtain_predicates = permissions.iter().map(|p| {
            vir::Stmt::Obtain(p.clone(), assert_pos) // TODO: Use a better position.
        });
        stmts.extend(obtain_predicates);

        stmts.push(vir::Stmt::Assert(
            func_spec.into_iter().conjoin(),
            vir::FoldingBehaviour::Expr,
            assert_pos,
        ));
        let equalities_expr = equalities.into_iter().conjoin();
        stmts.push(vir::Stmt::Assert(
            equalities_expr,
            vir::FoldingBehaviour::Expr,
            exhale_pos,
        ));
        let permission_expr = permissions.into_iter().conjoin();
        stmts.push(vir::Stmt::Exhale(permission_expr, exhale_pos));
        stmts
    }

    fn encode_loop_invariant_inhale_stmts(
        &mut self,
        loop_head: BasicBlockIndex,
        loop_inv_block: BasicBlockIndex,
        after_loop: bool,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_loop_invariant_inhale_stmts loop_head={:?} after_loop={}",
            loop_head,
            after_loop
        );
        let (permissions, equalities) =
            self.encode_loop_invariant_permissions(loop_head, loop_inv_block, true);
        let (func_spec, _func_spec_span) =
            self.encode_loop_invariant_specs(loop_head, loop_inv_block);

        let permission_expr = permissions.into_iter().conjoin();
        let equality_expr = equalities.into_iter().conjoin();

        let mut stmts = vec![vir::Stmt::comment(format!(
            "Inhale the loop invariant of block {:?}",
            loop_head
        ))];
        stmts.push(vir::Stmt::Inhale(
            permission_expr,
            vir::FoldingBehaviour::Stmt,
        ));
        stmts.push(vir::Stmt::Inhale(
            equality_expr,
            vir::FoldingBehaviour::Expr,
        ));
        stmts.push(vir::Stmt::Inhale(
            func_spec.into_iter().conjoin(),
            vir::FoldingBehaviour::Expr,
        ));
        stmts
    }

    // TODO: What is this?
    pub fn encode_prusti_local(&self, local: Local) -> vir::LocalVar {
        let var_name = self.locals.get_name(local);
        let type_name = self
            .encoder
            .encode_type_predicate_use(self.locals.get_type(local)).unwrap(); // will panic if attempting to encode unsupported type
        vir::LocalVar::new(var_name, vir::Type::TypedRef(type_name))
    }

    // /// Returns
    // /// - `vir::Expr`: the place of the projection;
    // /// - `ty::Ty<'tcx>`: the type of the place;
    // /// - `Option<usize>`: optionally, the variant of the enum.
    // fn encode_projection(
    //     &self,
    //     index: usize,
    //     place: mir::Place<'tcx>,
    //     root: Option<Local>,
    // ) -> (vir::Expr, ty::Ty<'tcx>, Option<usize>) {
    //     debug!("Encode projection {} {:?} {:?}", index, place, root);
    //     let encoded_place = self.encode_place_with_subst_root(&place_projection.base, root);
    //     self.mir_encoder
    //         .encode_projection(index, place, Some(encoded_place))
    // }

    /// `containing_def_id` – MIR body in which the place is defined. `location`
    /// `location` – MIR terminator that makes the function call. If None,
    /// then we assume that `containing_def_id` is local.
    pub fn encode_generic_place(
        &self,
        containing_def_id: rustc_hir::def_id::DefId,
        location: Option<mir::Location>,
        place: &Place<'tcx>,
    ) -> (vir::Expr, ty::Ty<'tcx>, Option<usize>) {
        let mir_encoder = if let Some(location) = location {
            let block = &self.mir.basic_blocks()[location.block];
            assert_eq!(block.statements.len(), location.statement_index, "expected terminator location");
            match &block.terminator().kind {
                mir::terminator::TerminatorKind::Call{ args, destination, .. } => {
                    let tcx = self.encoder.env().tcx();
                    let arg_tys = args.iter().map(|arg| arg.ty(self.mir, tcx)).collect();
                    let return_ty = destination.map(|(place, _)| place.ty(self.mir, tcx).ty);
                    FakeMirEncoder::new(self.encoder, arg_tys, return_ty)
                }
                kind => unreachable!("Only calls are expected. Found: {:?}", kind),
            }
        } else {
            let ref_mir = self.encoder.env().mir(containing_def_id.expect_local());
            let mir = ref_mir.borrow();
            let return_ty = mir.return_ty();
            let arg_tys = mir.args_iter().map(|arg| mir.local_decls[arg].ty).collect();
            FakeMirEncoder::new(self.encoder, arg_tys, Some(return_ty))
        };
        match place {
            Place::NormalPlace(place) => {
                mir_encoder.encode_place(place).unwrap()
            }
            Place::SubstitutedPlace {
                substituted_root,
                place
            } => {
                let (expr, ty, variant) = mir_encoder.encode_place(place).unwrap();
                let new_root = self.encode_prusti_local(*substituted_root);
                struct RootReplacer {
                    new_root: vir::LocalVar,
                }
                use prusti_common::vir::ExprFolder;
                impl ExprFolder for RootReplacer {
                    fn fold_local(&mut self, v: vir::LocalVar, p: vir::Position) -> vir::Expr {
                        Expr::Local(self.new_root.clone(), p)
                    }
                }
                (RootReplacer { new_root }.fold(expr), ty, variant)
            }
        }
        // match place {
        //     &Place::NormalPlace(ref place) => self.encode_place_with_subst_root(place, None),
        //     &Place::SubstitutedPlace {
        //         substituted_root,
        //         ref place,
        //     } => self.encode_place_with_subst_root(place, Some(substituted_root)),
        // }
    }

    // /// Returns
    // /// - `vir::Expr`: the expression of the projection;
    // /// - `ty::Ty<'tcx>`: the type of the expression;
    // /// - `Option<usize>`: optionally, the variant of the enum.
    // fn encode_place_with_subst_root(
    //     &self,
    //     place: &mir::Place<'tcx>,
    //     root: Option<Local>,
    // ) -> (vir::Expr, ty::Ty<'tcx>, Option<usize>) {
    //     if place.projection.is_empty() {
    //         let local = place.local;
    //         match root {
    //             Some(root) => (
    //                 self.encode_prusti_local(root).into(),
    //                 self.locals.get_type(root),
    //                 None,
    //             ),
    //             None => (
    //                 self.mir_encoder.encode_local(local).unwrap().into(), // will panic if attempting to encode unsupported type
    //                 self.mir_encoder.get_local_ty(local),
    //                 None,
    //             )
    //         }
    //     } else {
    //         self.encode_projection(place_projection, root)
    //     }
    //     // match place {
    //     //     &mir::Place::Local(local) => match root {
    //     //         Some(root) => (
    //     //             self.encode_prusti_local(root).into(),
    //     //             self.locals.get_type(root),
    //     //             None,
    //     //         ),
    //     //         None => (
    //     //             self.mir_encoder.encode_local(local).unwrap().into(), // will panic if attempting to encode unsupported type
    //     //             self.mir_encoder.get_local_ty(local),
    //     //             None,
    //     //         ),
    //     //     },
    //     //     &mir::Place::Projection(ref place_projection) => {
    //     //         self.encode_projection(place_projection, root)
    //     //     }
    //     //     x => unimplemented!("{:?}", x),
    //     // }
    // }

    /// Return type:
    /// - `Vec<vir::Stmt>`: the statements that encode the assignment of `operand` to `lhs`
    fn encode_assign_operand(
        &mut self,
        lhs: &vir::Expr,
        operand: &mir::Operand<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_assign_operand(lhs={}, operand={:?}, location={:?})",
            lhs, operand, location
        );
        let stmts = match operand {
            mir::Operand::Move(ref place) => {
                let (src, ty, _) = self.mir_encoder.encode_place(place).unwrap(); // will panic if attempting to encode unsupported type
                let mut stmts = match ty.kind() {
                    ty::TyKind::RawPtr(..) | ty::TyKind::Ref(..) => {
                        // Reborrow.
                        let field = self.encoder.encode_value_field(ty);
                        let mut alloc_stmts = self.prepare_assign_target(
                            lhs.clone(),
                            field.clone(),
                            location,
                            vir::AssignKind::Move,
                        );
                        alloc_stmts.push(vir::Stmt::Assign(
                            lhs.clone().field(field.clone()),
                            src.field(field),
                            vir::AssignKind::Move,
                        ));
                        alloc_stmts
                    }
                    _ => {
                        // Just move.
                        let move_assign =
                            vir::Stmt::Assign(lhs.clone(), src, vir::AssignKind::Move);
                        vec![move_assign]
                    }
                };

                // Store a label for this state
                let label = self.cfg_method.get_fresh_label_name();
                debug!("Current loc {:?} has label {}", location, label);
                self.label_after_location.insert(location, label.clone());
                stmts.push(vir::Stmt::Label(label.clone()));

                stmts
            }

            mir::Operand::Copy(ref place) => {
                let (src, ty, _) = self.mir_encoder.encode_place(place).unwrap(); // will panic if attempting to encode unsupported type

                let mut stmts = if self.mir_encoder.is_reference(ty) {
                    let loan = self.polonius_info().get_loan_at_location(location);
                    let ref_field = self.encoder.encode_value_field(ty);
                    let mut stmts = self.prepare_assign_target(
                        lhs.clone(),
                        ref_field.clone(),
                        location,
                        vir::AssignKind::SharedBorrow(loan.into()),
                    );
                    stmts.push(vir::Stmt::Assign(
                        lhs.clone().field(ref_field.clone()),
                        src.field(ref_field),
                        vir::AssignKind::SharedBorrow(loan.into()),
                    ));
                    stmts
                } else {
                    self.encode_copy2(src, lhs.clone(), ty, location)
                };

                // Store a label for this state
                let label = self.cfg_method.get_fresh_label_name();
                debug!("Current loc {:?} has label {}", location, label);
                self.label_after_location.insert(location, label.clone());
                stmts.push(vir::Stmt::Label(label.clone()));

                stmts
            }

            mir::Operand::Constant(box mir::Constant {
                literal: ty::Const { ty, val }, ..
            }) => {
                if let ty::TyKind::Tuple(elements) = ty.kind() {
                    // FIXME: This is most likley completely wrong. We need to
                    // implement proper support for handling constants of
                    // non-primitive types.
                    if !elements.is_empty() {
                        unimplemented!("Only ZSTs are currently supported, got: {:?}", elements);
                    }
                    // Since we have a ZST, we do not need to do anything to
                    // encode it.
                    Vec::new()
                } else {
                    // We expect to have a constant of a primitive type here.
                    let field = self.encoder.encode_value_field(ty);
                    let mut stmts = self.prepare_assign_target(
                        lhs.clone(),
                        field.clone(),
                        location,
                        vir::AssignKind::Copy,
                    );
                    // Initialize the constant
                    let const_val = self.encoder.encode_const_expr(*ty, val);
                    // Initialize value of lhs
                    stmts.push(vir::Stmt::Assign(
                        lhs.clone().field(field),
                        const_val,
                        vir::AssignKind::Copy,
                    ));
                    stmts
                }

                // FIXME: Delete the code below.
                // match literal {
                //     mir::Literal::Value { value } => {
                //         let const_val = self.encoder.encode_const_expr(value);
                //         // Initialize value of lhs
                //         stmts.push(vir::Stmt::Assign(
                //             lhs.clone().field(field),
                //             const_val,
                //             vir::AssignKind::Copy,
                //         ));
                //     }
                //     mir::Literal::Promoted { index } => {
                //         trace!("promoted constant literal {:?}: {:?}", index, ty);
                //         trace!("{:?}", self.mir.promoted[*index].basic_blocks());
                //         trace!(
                //             "{:?}",
                //             self.mir.promoted[*index]
                //                 .basic_blocks()
                //                 .into_iter()
                //                 .next()
                //                 .unwrap()
                //                 .statements[0]
                //         );
                //         // TODO: call eval_const
                //         debug!(
                //             "Encoding of promoted constant literal '{:?}: {:?}' is incomplete",
                //             index, ty
                //         );
                //         // Workaround: do not initialize values
                //     }
                // }
            }
        };
        debug!(
            "[enter] encode_assign_operand(lhs={}, operand={:?}, location={:?}) = {}",
            lhs,
            operand,
            location,
            vir::stmts_to_str(&stmts)
        );
        stmts
    }

    fn encode_assign_binary_op(
        &mut self,
        op: mir::BinOp,
        left: &mir::Operand<'tcx>,
        right: &mir::Operand<'tcx>,
        encoded_lhs: vir::Expr,
        ty: ty::Ty<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_assign_binary_op(op={:?}, left={:?}, right={:?})",
            op,
            left,
            right
        );
        let encoded_left = self.mir_encoder.encode_operand_expr(left);
        let encoded_right = self.mir_encoder.encode_operand_expr(right);
        let encoded_value =
            self.mir_encoder
                .encode_bin_op_expr(op, encoded_left, encoded_right, ty);
        self.encode_copy_value_assign(encoded_lhs, encoded_value, ty, location)
    }

    fn encode_copy_value_assign(
        &mut self,
        encoded_lhs: vir::Expr,
        encoded_rhs: vir::Expr,
        ty: ty::Ty<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        let field = self.encoder.encode_value_field(ty);
        self.encode_copy_value_assign2(encoded_lhs, encoded_rhs, field, location)
    }

    fn encode_assign_checked_binary_op(
        &mut self,
        op: mir::BinOp,
        left: &mir::Operand<'tcx>,
        right: &mir::Operand<'tcx>,
        encoded_lhs: vir::Expr,
        ty: ty::Ty<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_assign_checked_binary_op(op={:?}, left={:?}, right={:?})",
            op,
            left,
            right
        );
        let operand_ty = if let ty::TyKind::Tuple(ref types) = ty.kind() {
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
            operand_ty.expect_ty(),
        );
        let encoded_check =
            self.mir_encoder
                .encode_bin_op_check(op, encoded_left, encoded_right, operand_ty.expect_ty());
        let field_types = if let ty::TyKind::Tuple(ref x) = ty.kind() {
            x
        } else {
            unreachable!()
        };
        let value_field = self
            .encoder
            .encode_raw_ref_field("tuple_0".to_string(), field_types[0].expect_ty());
        let value_field_value = self.encoder.encode_value_field(field_types[0].expect_ty());
        let check_field = self
            .encoder
            .encode_raw_ref_field("tuple_1".to_string(), field_types[1].expect_ty());
        let check_field_value = self.encoder.encode_value_field(field_types[1].expect_ty());
        let mut stmts = if !self
            .init_info
            .is_vir_place_accessible(&encoded_lhs, location)
        {
            let mut alloc_stmts = self.encode_havoc(&encoded_lhs);
            let mut inhale_acc = |place| {
                alloc_stmts.push(vir::Stmt::Inhale(
                    vir::Expr::acc_permission(place, vir::PermAmount::Write),
                    vir::FoldingBehaviour::Stmt,
                ));
            };
            inhale_acc(encoded_lhs.clone().field(value_field.clone()));
            inhale_acc(
                encoded_lhs
                    .clone()
                    .field(value_field.clone())
                    .field(value_field_value.clone()),
            );
            inhale_acc(encoded_lhs.clone().field(check_field.clone()));
            inhale_acc(
                encoded_lhs
                    .clone()
                    .field(check_field.clone())
                    .field(check_field_value.clone()),
            );
            alloc_stmts
        } else {
            Vec::with_capacity(2)
        };
        // Initialize lhs.field
        stmts.push(vir::Stmt::Assign(
            encoded_lhs
                .clone()
                .field(value_field)
                .field(value_field_value),
            encoded_value,
            vir::AssignKind::Copy,
        ));
        stmts.push(vir::Stmt::Assign(
            encoded_lhs.field(check_field).field(check_field_value),
            encoded_check,
            vir::AssignKind::Copy,
        ));
        stmts
    }

    fn encode_assign_unary_op(
        &mut self,
        op: mir::UnOp,
        operand: &mir::Operand<'tcx>,
        encoded_lhs: vir::Expr,
        ty: ty::Ty<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_assign_unary_op(op={:?}, operand={:?})",
            op,
            operand
        );
        let encoded_val = self.mir_encoder.encode_operand_expr(operand);
        let encoded_value = self.mir_encoder.encode_unary_op_expr(op, encoded_val);
        // Initialize `lhs.field`
        self.encode_copy_value_assign(encoded_lhs, encoded_value, ty, location)
    }

    fn encode_assign_nullary_op(
        &mut self,
        op: mir::NullOp,
        op_ty: ty::Ty<'tcx>,
        encoded_lhs: vir::Expr,
        ty: ty::Ty<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_assign_nullary_op(op={:?}, op_ty={:?})",
            op,
            op_ty
        );
        match op {
            mir::NullOp::Box => {
                assert_eq!(op_ty, ty.boxed_ty());
                let ref_field = self.encoder.encode_dereference_field(op_ty);
                let box_content = encoded_lhs.clone().field(ref_field.clone());

                let mut stmts = self.prepare_assign_target(
                    encoded_lhs,
                    ref_field,
                    location,
                    vir::AssignKind::Move,
                );

                // Allocate `box_content`
                stmts.extend(self.encode_havoc_and_allocation(&box_content));

                // Leave `box_content` uninitialized
                stmts
            }
            mir::NullOp::SizeOf => unimplemented!(),
        }
    }

    fn encode_assign_discriminant(
        &mut self,
        src: &mir::Place<'tcx>,
        location: mir::Location,
        encoded_lhs: vir::Expr,
        ty: ty::Ty<'tcx>,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_assign_discriminant(src={:?}, location={:?})",
            src,
            location
        );
        let (encoded_src, src_ty, _) = self.mir_encoder.encode_place(src).unwrap(); // will panic if attempting to encode unsupported type
        match src_ty.kind() {
            ty::TyKind::Adt(ref adt_def, _) if !adt_def.is_box() => {
                let num_variants = adt_def.variants.len();
                // Initialize `lhs.int_field`
                // Note: in our encoding an enumeration with just one variant has
                // no discriminant
                if num_variants > 1 {
                    let encoded_rhs = self.encoder.encode_discriminant_func_app(
                        self.translate_maybe_borrowed_place(location, encoded_src),
                        adt_def,
                    );
                    self.encode_copy_value_assign(encoded_lhs.clone(), encoded_rhs, ty, location)
                } else {
                    vec![]
                }
            }

            ty::TyKind::Int(_) | ty::TyKind::Uint(_) => {
                let value_field = self.encoder.encode_value_field(src_ty);
                let discr_value: vir::Expr =
                    self.translate_maybe_borrowed_place(location, encoded_src.field(value_field));
                self.encode_copy_value_assign(encoded_lhs.clone(), discr_value, ty, location)
            }

            ref x => {
                debug!("The discriminant of type {:?} is not defined", x);
                vec![]
            }
        }
    }

    fn encode_assign_ref(
        &mut self,
        mir_borrow_kind: mir::BorrowKind,
        place: &mir::Place<'tcx>,
        location: mir::Location,
        encoded_lhs: vir::Expr,
        ty: ty::Ty<'tcx>,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_assign_ref(mir_borrow_kind={:?}, place={:?}, location={:?})",
            mir_borrow_kind,
            place,
            location
        );
        let (encoded_value, _, _) = self.mir_encoder.encode_place(place).unwrap(); // will panic if attempting to encode unsupported type
        let loan = self.polonius_info().get_loan_at_location(location);
        let vir_assign_kind = match mir_borrow_kind {
            mir::BorrowKind::Shared => vir::AssignKind::SharedBorrow(loan.into()),
            mir::BorrowKind::Unique => unimplemented!(),
            mir::BorrowKind::Shallow => unimplemented!(),
            mir::BorrowKind::Mut { .. } => vir::AssignKind::MutableBorrow(loan.into()),
        };
        // Initialize ref_var.ref_field
        let field = self.encoder.encode_value_field(ty);
        let mut stmts = self.prepare_assign_target(
            encoded_lhs.clone(),
            field.clone(),
            location,
            vir_assign_kind,
        );
        stmts.push(vir::Stmt::Assign(
            encoded_lhs.field(field),
            encoded_value,
            vir_assign_kind,
        ));
        // Store a label for this state
        let label = self.cfg_method.get_fresh_label_name();
        debug!("Current loc {:?} has label {}", location, label);
        self.label_after_location.insert(location, label.clone());
        stmts.push(vir::Stmt::Label(label.clone()));
        stmts
    }

    fn encode_cast(
        &mut self,
        operand: &mir::Operand<'tcx>,
        dst_ty: ty::Ty<'tcx>,
        encoded_lhs: vir::Expr,
        ty: ty::Ty<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] encode_cast(operand={:?}, dst_ty={:?})",
            operand,
            dst_ty
        );
        let encoded_val = self.mir_encoder.encode_cast_expr(operand, dst_ty);
        self.encode_copy_value_assign(encoded_lhs, encoded_val, ty, location)
    }

    pub fn get_auxiliary_local_var(&mut self, suffix: &str, vir_type: vir::Type) -> vir::LocalVar {
        let name = format!("_aux_{}_{}", suffix, vir_type.name());
        if self.auxiliary_local_vars.contains_key(&name) {
            assert_eq!(self.auxiliary_local_vars[&name], vir_type);
        } else {
            self.cfg_method.add_local_var(&name, vir_type.clone());
            self.auxiliary_local_vars
                .insert(name.clone(), vir_type.clone());
        }
        vir::LocalVar::new(name, vir_type)
    }

    fn encode_havoc(&mut self, dst: &vir::Expr) -> Vec<vir::Stmt> {
        debug!("Encode havoc {:?}", dst);
        let havoc_ref_method_name = self
            .encoder
            .encode_builtin_method_use(BuiltinMethodKind::HavocRef);
        if let &vir::Expr::Local(ref dst_local_var, ref _pos) = dst {
            vec![vir::Stmt::MethodCall(
                havoc_ref_method_name,
                vec![],
                vec![dst_local_var.clone()],
            )]
        } else {
            let tmp_var = self.get_auxiliary_local_var("havoc", dst.get_type().clone());
            vec![
                vir::Stmt::MethodCall(havoc_ref_method_name, vec![], vec![tmp_var.clone()]),
                vir::Stmt::Assign(dst.clone().into(), tmp_var.into(), vir::AssignKind::Move),
            ]
        }
    }

    /// Havoc and assume permission on fields
    fn encode_havoc_and_allocation(&mut self, dst: &vir::Expr) -> Vec<vir::Stmt> {
        debug!("Encode havoc and allocation {:?}", dst);

        let mut stmts = vec![];
        // Havoc `dst`
        stmts.extend(self.encode_havoc(dst));
        // Allocate `dst`
        stmts.push(vir::Stmt::Inhale(
            self.mir_encoder
                .encode_place_predicate_permission(dst.clone(), vir::PermAmount::Write)
                .unwrap(),
            vir::FoldingBehaviour::Stmt,
        ));
        stmts
    }

    /// Prepare the ``dst`` to be copy target:
    ///
    /// 1.  Havoc and allocate if it is not yet allocated.
    fn prepare_assign_target(
        &mut self,
        dst: vir::Expr,
        field: vir::Field,
        location: mir::Location,
        vir_assign_kind: vir::AssignKind,
    ) -> Vec<vir::Stmt> {
        trace!(
            "[enter] prepare_assign_target(dst={}, field={}, location={:?})",
            dst,
            field,
            location
        );
        if !self.init_info.is_vir_place_accessible(&dst, location) {
            let mut alloc_stmts = self.encode_havoc(&dst);
            let dst_field = dst.clone().field(field.clone());
            let acc = vir::Expr::acc_permission(dst_field, vir::PermAmount::Write);
            alloc_stmts.push(vir::Stmt::Inhale(acc, vir::FoldingBehaviour::Stmt));
            match vir_assign_kind {
                vir::AssignKind::Copy => {
                    if field.typ.is_ref() {
                        unimplemented!("Inhale the predicate rooted at dst_field.");
                    }
                }
                vir::AssignKind::Move
                | vir::AssignKind::MutableBorrow(_)
                | vir::AssignKind::SharedBorrow(_) => {}
                vir::AssignKind::Ghost => unreachable!(),
            }
            debug!("alloc_stmts = {}", alloc_stmts.iter().to_string());
            alloc_stmts
        } else {
            Vec::with_capacity(1)
        }
    }

    /// Encode value copy assignment. Havoc and allocate the target if necessary.
    fn encode_copy_value_assign2(
        &mut self,
        lhs: vir::Expr,
        rhs: vir::Expr,
        field: vir::Field,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        let mut stmts =
            self.prepare_assign_target(lhs.clone(), field.clone(), location, vir::AssignKind::Copy);
        stmts.push(vir::Stmt::Assign(
            lhs.field(field),
            rhs,
            vir::AssignKind::Copy,
        ));
        stmts
    }

    /// Copy a primitive value such as an integer. Allocate the target
    /// if necessary.
    fn encode_copy_primitive_value(
        &mut self,
        src: vir::Expr,
        dst: vir::Expr,
        ty: ty::Ty<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        let field = self.encoder.encode_value_field(ty);
        self.encode_copy_value_assign2(dst, src.field(field.clone()), field, location)
    }

    fn encode_deep_copy_adt(
        &mut self,
        src: vir::Expr,
        dst: vir::Expr,
        self_ty: ty::Ty<'tcx>,
    ) -> Vec<vir::Stmt> {
        let mut stmts = self.encode_havoc(&dst);
        let pred = vir::Expr::pred_permission(dst.clone(), vir::PermAmount::Write).unwrap();
        stmts.push(vir::Stmt::Inhale(pred, vir::FoldingBehaviour::Stmt));
        let eq =
            self.encoder
                .encode_memory_eq_func_app(src, dst, self_ty, vir::Position::default());
        stmts.push(vir::Stmt::Inhale(eq, vir::FoldingBehaviour::Stmt));
        stmts
    }

    fn encode_deep_copy_tuple(
        &mut self,
        src: vir::Expr,
        dst: vir::Expr,
        elems: ty::subst::SubstsRef<'tcx>,
    ) -> Vec<vir::Stmt> {
        let mut stmts = self.encode_havoc(&dst);
        for (field_num, arg) in elems.iter().enumerate() {
            let ty = arg.expect_ty();
            let field_name = format!("tuple_{}", field_num);
            let field = self.encoder.encode_raw_ref_field(field_name, ty);
            let dst_field = dst.clone().field(field.clone());
            let acc = vir::Expr::acc_permission(dst_field.clone(), vir::PermAmount::Write);
            let pred =
                vir::Expr::pred_permission(dst_field.clone(), vir::PermAmount::Write).unwrap();
            stmts.push(vir::Stmt::Inhale(acc, vir::FoldingBehaviour::Stmt));
            stmts.push(vir::Stmt::Inhale(pred, vir::FoldingBehaviour::Stmt));
            let src_field = src.clone().field(field.clone());
            let eq = self.encoder.encode_memory_eq_func_app(
                src_field,
                dst_field,
                ty,
                vir::Position::default(),
            );
            stmts.push(vir::Stmt::Inhale(eq, vir::FoldingBehaviour::Stmt));
        }
        stmts
    }

    fn encode_copy2(
        &mut self,
        src: vir::Expr,
        dst: vir::Expr,
        self_ty: ty::Ty<'tcx>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        let stmts = match self_ty.kind() {
            ty::TyKind::Bool
            | ty::TyKind::Int(_)
            | ty::TyKind::Uint(_)
            | ty::TyKind::Char => {
                self.encode_copy_primitive_value(src, dst, self_ty, location)
            }
            ty::TyKind::Adt(adt_def, _subst) if !adt_def.is_box() => {
                self.encode_deep_copy_adt(src, dst, self_ty)
            }
            ty::TyKind::Tuple(elems) => self.encode_deep_copy_tuple(src, dst, elems),
            ty::TyKind::Param(_) => {
                let mut stmts = self.encode_havoc_and_allocation(&dst.clone());
                let eq = self.encoder.encode_memory_eq_func_app(
                    src,
                    dst,
                    self_ty,
                    vir::Position::default(),
                );
                stmts.push(vir::Stmt::Inhale(eq, vir::FoldingBehaviour::Stmt));
                stmts
            }

            ref x => unimplemented!("{:?}", x),
        };
        stmts
    }

    fn encode_assign_aggregate(
        &mut self,
        dst: &vir::Expr,
        ty: ty::Ty<'tcx>,
        aggregate: &mir::AggregateKind<'tcx>,
        operands: &Vec<mir::Operand<'tcx>>,
        location: mir::Location,
    ) -> Vec<vir::Stmt> {
        debug!(
            "[enter] encode_assign_aggregate({:?}, {:?})",
            aggregate, operands
        );
        let mut stmts = self.encode_havoc_and_allocation(dst);
        // Initialize values
        match aggregate {
            &mir::AggregateKind::Tuple => {
                let field_types = if let ty::TyKind::Tuple(ref x) = ty.kind() {
                    x
                } else {
                    unreachable!()
                };
                for (field_num, operand) in operands.iter().enumerate() {
                    let field_name = format!("tuple_{}", field_num);
                    let encoded_field = self
                        .encoder
                        .encode_raw_ref_field(field_name, field_types[field_num].expect_ty());
                    stmts.extend(self.encode_assign_operand(
                        &dst.clone().field(encoded_field),
                        operand,
                        location,
                    ));
                }
                stmts
            }

            &mir::AggregateKind::Adt(adt_def, variant_index, subst, _, _) => {
                let num_variants = adt_def.variants.len();
                let variant_def = &adt_def.variants[variant_index];
                let mut dst_base = dst.clone();
                if num_variants != 1 {
                    // An enum.
                    let tcx = self.encoder.env().tcx();
                    // Handle *signed* discriminats
                    let discr_value: vir::Expr = if let SignedInt(ity) = adt_def.repr.discr_type() {
                        let bit_size =
                            Integer::from_attr(&self.encoder.env().tcx(), SignedInt(ity))
                                .size()
                                .bits();
                        let shift = 128 - bit_size;
                        let unsigned_discr =
                            adt_def.discriminant_for_variant(tcx, variant_index).val;
                        let casted_discr = unsigned_discr as i128;
                        // sign extend the raw representation to be an i128
                        ((casted_discr << shift) >> shift).into()
                    } else {
                        adt_def
                            .discriminant_for_variant(tcx, variant_index)
                            .val
                            .into()
                    };
                    // dst was havocked, so it is safe to assume the equality here.
                    let discriminant = self
                        .encoder
                        .encode_discriminant_func_app(dst.clone(), adt_def);
                    stmts.push(vir::Stmt::Inhale(
                        vir::Expr::eq_cmp(discriminant, discr_value),
                        vir::FoldingBehaviour::Stmt,
                    ));
                    let variant_name = &variant_def.ident.as_str();
                    dst_base = dst_base.variant(variant_name);
                }
                for (field_index, field) in variant_def.fields.iter().enumerate() {
                    let operand = &operands[field_index];
                    let field_name = &field.ident.as_str();
                    let tcx = self.encoder.env().tcx();
                    let field_ty = field.ty(tcx, subst);
                    let encoded_field = self.encoder.encode_struct_field(field_name, field_ty);
                    stmts.extend(self.encode_assign_operand(
                        &dst_base.clone().field(encoded_field),
                        operand,
                        location,
                    ));
                }
                stmts
            }

            &mir::AggregateKind::Closure(def_id, _substs) => {
                //assert!(self.encoder.is_spec_closure(def_id), "closure: {:?}", def_id);
                // Specification only. Just ignore in the encoding.
                // FIXME: Filtering of specification blocks is broken, so we need to handle this here.
              if self.encoder.is_spec_closure(def_id) {
                // Specification only. Just ignore in the encoding.
                // FIXME: Filtering of specification blocks is broken, so we need to handle this here.
                Vec::new()
                } else {
                  unimplemented!();
                }
            }

            ref x => unimplemented!("{:?}", x),
        }
    }

    fn check_vir(&self) -> Result<()> {
        if self.cfg_method.has_loops() {
            return Err(EncodingError::internal(
                "The Viper encoding contains unexpected loops in the CFG",
                self.mir.span,
            ));
        }
        Ok(())
    }

    fn get_label_after_location(&mut self, location: mir::Location) -> &str {
        debug_assert!(
            self.label_after_location.contains_key(&location),
            "Location {:?} has not been encoded yet",
            location
        );
        &self.label_after_location[&location]
    }

    fn get_loop_span(&self, loop_head: mir::BasicBlock) -> Span {
        let loop_info = self.loop_encoder.loops();
        debug_assert!(loop_info.is_loop_head(loop_head));
        let loop_body = loop_info.get_loop_body(loop_head);
        let loop_head_span = self.mir_encoder.get_span_of_basic_block(loop_head);
        loop_body
            .iter()
            .map(|&bb| self.mir_encoder.get_span_of_basic_block(bb))
            .filter(|&span| span.contains(loop_head_span))
            .min()
            .unwrap()
    }
}

fn convert_loans_to_borrows(loans: &Vec<facts::Loan>) -> Vec<Borrow> {
    loans.iter().map(|l| l.into()).collect()
}
