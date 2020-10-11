// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use self::path_ctxt::*;
use crate::encoder::foldunfold::action::Action;
use crate::encoder::foldunfold::perm::*;
use crate::encoder::foldunfold::permissions::*;
use crate::encoder::foldunfold::semantics::ApplyOnState;
use crate::encoder::Encoder;
use prusti_common::utils::to_string::ToString;
use prusti_common::vir;
use prusti_common::vir::borrows::Borrow;
use prusti_common::vir::{CfgBlockIndex, CfgReplacer, CheckNoOpAction};
use prusti_common::vir::{ExprFolder, FallibleExprFolder, PermAmount};
use prusti_common::config;
use prusti_common::report;
use rustc_middle::mir;
use std;
use std::collections::{HashMap, HashSet};
use std::mem;
use std::ops::Deref;
use ::log::{trace, debug};

mod action;
mod borrows;
mod log;
mod path_ctxt;
mod perm;
mod permissions;
mod places_utils;
mod semantics;
mod state;
mod process_expire_borrows;

#[derive(Clone, Debug)]
pub enum FoldUnfoldError {
    /// The algorithm failed to obtain a permission
    FailedToObtain(Perm),
    /// The algorithm tried to generate a "folding .. in .." Viper expression
    RequiresFolding(
        String,
        Vec<vir::Expr>,
        PermAmount,
        vir::MaybeEnumVariantIndex,
        vir::Position,
    ),
}

pub fn add_folding_unfolding_to_expr(
    expr: vir::Expr,
    pctxt: &PathCtxt,
) -> Result<vir::Expr, FoldUnfoldError> {
    let pctxt_at_label = HashMap::new();
    let expr = ExprReplacer::new(pctxt.clone(), &pctxt_at_label, true).fallible_fold(expr)?;
    ExprReplacer::new(pctxt.clone(), &pctxt_at_label, false).fallible_fold(expr)
}

pub fn add_folding_unfolding_to_function(
    function: vir::Function,
    predicates: HashMap<String, vir::Predicate>,
) -> Result<vir::Function, FoldUnfoldError> {
    // Compute inner state
    let formal_vars = function.formal_args.clone();
    let mut pctxt = PathCtxt::new(formal_vars, &predicates);
    for pre in &function.pres {
        pctxt.apply_stmt(&vir::Stmt::Inhale(pre.clone(), vir::FoldingBehaviour::Expr));
    }
    // Add appropriate unfolding around expressions
    let result = Ok(vir::Function {
        pres: function
            .pres
            .into_iter()
            .map(|e| add_folding_unfolding_to_expr(e, &pctxt))
            .collect::<Result<_, FoldUnfoldError>>()?,
        posts: function
            .posts
            .into_iter()
            .map(|e| add_folding_unfolding_to_expr(e, &pctxt))
            .collect::<Result<_, FoldUnfoldError>>()?,
        body: function
            .body
            .map(|e| add_folding_unfolding_to_expr(e, &pctxt))
            .map_or(Ok(None), |r| r.map(Some))?,
        ..function
    });
    result
}

pub fn add_fold_unfold<'p, 'v: 'p, 'tcx: 'v>(
    encoder: &'p Encoder<'v, 'tcx>,
    cfg: vir::CfgMethod,
    borrow_locations: &'p HashMap<Borrow, mir::Location>,
    cfg_map: &'p HashMap<mir::BasicBlock, HashSet<CfgBlockIndex>>,
    method_pos: vir::Position,
) -> Result<vir::CfgMethod, FoldUnfoldError> {
    let cfg_vars = cfg.get_all_vars();
    let predicates = encoder.get_used_viper_predicates_map();
    let initial_pctxt = PathCtxt::new(cfg_vars, &predicates);
    FoldUnfold::new(
        encoder,
        initial_pctxt,
        &cfg,
        borrow_locations,
        cfg_map,
        method_pos,
    )
    .replace_cfg(&cfg)
}

#[derive(Clone)]
struct FoldUnfold<'p, 'v: 'p, 'tcx: 'v> {
    encoder: &'p Encoder<'v, 'tcx>,
    initial_pctxt: PathCtxt<'p>,
    pctxt_at_label: HashMap<String, PathCtxt<'p>>,
    dump_debug_info: bool,
    /// Used for debugging the dump
    foldunfold_state_filter: String,
    check_foldunfold_state: bool,
    cfg: &'p vir::CfgMethod,
    borrow_locations: &'p HashMap<vir::borrows::Borrow, mir::Location>,
    cfg_map: &'p HashMap<mir::BasicBlock, HashSet<CfgBlockIndex>>,
    method_pos: vir::Position,
}

impl<'p, 'v: 'p, 'tcx: 'v> FoldUnfold<'p, 'v, 'tcx> {
    pub fn new(
        encoder: &'p Encoder<'v, 'tcx>,
        initial_pctxt: PathCtxt<'p>,
        cfg: &'p vir::CfgMethod,
        borrow_locations: &'p HashMap<vir::borrows::Borrow, mir::Location>,
        cfg_map: &'p HashMap<mir::BasicBlock, HashSet<CfgBlockIndex>>,
        method_pos: vir::Position,
    ) -> Self {
        FoldUnfold {
            encoder,
            initial_pctxt,
            pctxt_at_label: HashMap::new(),
            dump_debug_info: config::dump_debug_info(),
            check_foldunfold_state: config::check_foldunfold_state(),
            foldunfold_state_filter: config::foldunfold_state_filter(),
            cfg,
            borrow_locations,
            cfg_map,
            method_pos,
        }
    }

    fn replace_expr(
        &self,
        expr: &vir::Expr,
        curr_pctxt: &PathCtxt<'p>,
    ) -> Result<vir::Expr, FoldUnfoldError> {
        ExprReplacer::new(curr_pctxt.clone(), &self.pctxt_at_label, false)
            .fallible_fold(expr.clone())
    }

    fn replace_old_expr(
        &self,
        expr: &vir::Expr,
        curr_pctxt: &PathCtxt<'p>,
    ) -> Result<vir::Expr, FoldUnfoldError> {
        ExprReplacer::new(curr_pctxt.clone(), &self.pctxt_at_label, true)
            .fallible_fold(expr.clone())
    }

    /// Insert "unfolding in" in old expressions
    fn rewrite_stmt_with_unfoldings_in_old(
        &self,
        stmt: vir::Stmt,
        pctxt: &PathCtxt<'p>,
    ) -> Result<vir::Stmt, FoldUnfoldError> {
        trace!("[enter] rewrite_stmt_with_unfoldings_in_old: {}", stmt);
        let result = match stmt {
            vir::Stmt::PackageMagicWand(magic_wand, stmts, label, vars, pos) => {
                let magic_wand = self.replace_old_expr(&magic_wand, pctxt)?;
                vir::Stmt::PackageMagicWand(magic_wand, stmts, label, vars, pos)
            }
            vir::Stmt::If(cond, then, elze) => {
                let cond = self.replace_old_expr(&cond, pctxt)?;
                vir::Stmt::If(cond, then, elze)
            }
            _ => stmt.fallible_map_expr(|e| self.replace_old_expr(&e, pctxt))?
        };
        trace!("[exit] rewrite_stmt_with_unfoldings_in_old = {}", result);
        Ok(result)
    }

    /// Insert "unfolding in" expressions
    fn rewrite_stmt_with_unfoldings(
        &self,
        stmt: vir::Stmt,
        pctxt: &PathCtxt<'p>,
    ) -> Result<vir::Stmt, FoldUnfoldError> {
        match stmt {
            vir::Stmt::Inhale(expr, folding) => {
                // Compute inner state
                let mut inner_pctxt = pctxt.clone();
                let inner_state = inner_pctxt.mut_state();
                inner_state.insert_all_perms(
                    expr.get_permissions(pctxt.predicates())
                        .into_iter()
                        .filter(|p| !p.get_place().is_local() && p.is_curr()),
                );

                // Rewrite statement
                Ok(vir::Stmt::Inhale(
                    self.replace_expr(&expr, &inner_pctxt)?,
                    folding,
                ))
            }
            vir::Stmt::TransferPerm(lhs, rhs, unchecked) => {
                // Compute rhs state
                let rhs_pctxt = pctxt.clone();
                /*
                let rhs_state = rhs_pctxt.mut_state();
                rhs_state.insert_all_perms(
                    rhs.get_permissions(pctxt.predicates())
                        .into_iter()
                        .filter(|p| p.is_pred())
                );
                */
                let new_lhs = if unchecked {
                    lhs
                } else {
                    self.replace_expr(&lhs, &pctxt)?
                };

                // Rewrite statement
                Ok(vir::Stmt::TransferPerm(
                    new_lhs,
                    self.replace_old_expr(&rhs, &rhs_pctxt)?,
                    unchecked,
                ))
            }
            vir::Stmt::PackageMagicWand(wand, stmts, label, vars, pos) => {
                Ok(vir::Stmt::PackageMagicWand(
                    self.replace_expr(&wand, pctxt)?,
                    stmts,
                    label,
                    vars,
                    pos,
                ))
            }
            vir::Stmt::If(cond, then, elze) => {
                let cond = self.replace_expr(&cond, pctxt)?;
                Ok(vir::Stmt::If(cond, then, elze))
            }
            _ => stmt.fallible_map_expr(|e| self.replace_expr(&e, pctxt)),
        }
    }

    fn get_cfg_block_of_last_borrow(
        &self,
        curr_block: CfgBlockIndex,
        borrow: &Borrow,
    ) -> CfgBlockIndex {
        let mir_location = self.borrow_locations[borrow];
        let borrow_creation = &self.cfg_map[&mir_location.block];
        // HACK: Choose the closest block. Can be optimized.
        let mut nearest_block = None;
        for &block in borrow_creation {
            if let Some(path) = self.cfg.find_path(block, curr_block) {
                if let Some((_, distance)) = nearest_block {
                    if distance > path.len() {
                        nearest_block = Some((block, path.len()));
                    }
                } else {
                    nearest_block = Some((block, path.len()));
                }
            }
        }
        assert!(
            nearest_block.is_some(),
            "Could not find a predecessor of {:?} in the blocks that create the borrow {:?} ({:?})",
            curr_block,
            borrow,
            borrow_creation,
        );
        nearest_block.unwrap().0
    }

    /// Restore `Write` permissions that were converted to `Read` due to borrowing.
    fn restore_write_permissions(
        &self,
        borrow: vir::borrows::Borrow,
        pctxt: &mut PathCtxt,
    ) -> Result<Vec<vir::Stmt>, FoldUnfoldError> {
        trace!("[enter] restore_write_permissions({:?})", borrow);
        let mut stmts = Vec::new();
        for access in pctxt.log().get_converted_to_read_places(borrow) {
            trace!("restore_write_permissions access={}", access);
            let perm = match access {
                vir::Expr::PredicateAccessPredicate(_, box ref arg, perm_amount, _) => {
                    assert!(perm_amount == vir::PermAmount::Remaining);
                    Perm::pred(arg.clone(), vir::PermAmount::Read)
                }
                vir::Expr::FieldAccessPredicate(box ref place, perm_amount, _) => {
                    assert!(perm_amount == vir::PermAmount::Remaining);
                    Perm::acc(place.clone(), vir::PermAmount::Read)
                }
                x => unreachable!("{:?}", x),
            };
            stmts.extend(
                pctxt
                    .obtain_permissions(vec![perm])?
                    .iter()
                    .map(|a| a.to_stmt()),
            );
            let inhale_stmt = vir::Stmt::Inhale(access, vir::FoldingBehaviour::Stmt);
            pctxt.apply_stmt(&inhale_stmt);
            stmts.push(inhale_stmt);
        }

        // Log borrow expiration
        pctxt.log_mut().log_borrow_expiration(borrow);

        trace!(
            "[exit] restore_write_permissions({:?}) = {}",
            borrow,
            vir::stmts_to_str(&stmts)
        );
        Ok(stmts)
    }

    /// Wrap `_1.val_ref.f.g.` into `old[label](_1.val_ref).f.g`. This is needed
    /// to make `_1.val_ref` reachable inside a package statement of a magic wand.
    fn patch_places(&self, stmts: &Vec<vir::Stmt>, maybe_label: Option<&str>) -> Vec<vir::Stmt> {
        if let Some(label) = maybe_label {
            struct PlacePatcher<'a> {
                label: &'a str,
            }
            impl<'a> vir::ExprFolder for PlacePatcher<'a> {
                fn fold(&mut self, e: vir::Expr) -> vir::Expr {
                    match e {
                        vir::Expr::Field(box vir::Expr::Local(_, _), _, _) => e.old(self.label),
                        _ => vir::default_fold_expr(self, e),
                    }
                }
                fn fold_labelled_old(
                    &mut self,
                    label: String,
                    expr: Box<vir::Expr>,
                    pos: vir::Position,
                ) -> vir::Expr {
                    // Do not replace places that are already old.
                    vir::Expr::LabelledOld(label, expr, pos)
                }
            }
            fn patch_expr(label: &str, expr: &vir::Expr) -> vir::Expr {
                PlacePatcher { label }.fold(expr.clone())
            }
            fn patch_args(label: &str, args: &Vec<vir::Expr>) -> Vec<vir::Expr> {
                args.iter()
                    .map(|arg| {
                        assert!(arg.is_place());
                        patch_expr(label, arg)
                    })
                    .collect()
            }
            stmts
                .iter()
                .map(|stmt| match stmt {
                    vir::Stmt::Comment(_)
                    | vir::Stmt::ApplyMagicWand(_, _)
                    | vir::Stmt::TransferPerm(_, _, _)
                    | vir::Stmt::Assign(_, _, _) => stmt.clone(),
                    vir::Stmt::Inhale(expr, folding) => {
                        vir::Stmt::Inhale(patch_expr(label, expr), *folding)
                    }
                    vir::Stmt::Exhale(expr, pos) => {
                        vir::Stmt::Exhale(patch_expr(label, expr), *pos)
                    }
                    vir::Stmt::Fold(ref pred_name, ref args, perm_amount, variant, pos) => {
                        vir::Stmt::Fold(
                            pred_name.clone(),
                            patch_args(label, args),
                            *perm_amount,
                            variant.clone(),
                            *pos,
                        )
                    }
                    vir::Stmt::Unfold(ref pred_name, ref args, perm_amount, variant) => {
                        vir::Stmt::Unfold(
                            pred_name.clone(),
                            patch_args(label, args),
                            *perm_amount,
                            variant.clone(),
                        )
                    }
                    vir::Stmt::Label(_) => stmt.clone(),
                    x => unreachable!("{}", x),
                })
                .collect()
        } else {
            stmts.clone()
        }
    }
}

#[derive(Debug)]
struct ActionVec(pub Vec<Action>);

impl Deref for ActionVec {
    type Target = Vec<Action>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// TODO: get rid of newtype wrapper when rust updated to 1.41.0+, where the orphan rules are relaxed to allow things like this
impl CheckNoOpAction for ActionVec {
    fn is_noop(&self) -> bool {
        self.is_empty()
    }
}

impl<'p, 'v: 'p, 'tcx: 'v> vir::CfgReplacer<PathCtxt<'p>, ActionVec>
    for FoldUnfold<'p, 'v, 'tcx>
{
    type Error = FoldUnfoldError;

    /// Dump the current CFG, for debugging purposes
    fn current_cfg(
        &self,
        new_cfg: &vir::CfgMethod,
        initial_pctxt: &[Option<PathCtxt>],
        _final_pctxt: &[Option<PathCtxt>],
    ) {
        if self.dump_debug_info {
            let source_path = self.encoder.env().source_path();
            let source_filename = source_path.file_name().unwrap().to_str().unwrap();
            let method_name = new_cfg.name();
            report::log::report_with_writer(
                "graphviz_method_during_foldunfold",
                format!("{}.{}.dot", source_filename, method_name),
                |writer| {
                    new_cfg.to_graphviz_with_extra(writer, |bb_index| {
                        initial_pctxt
                            .get(bb_index)
                            .and_then(|opt_pctxt| {
                                opt_pctxt.as_ref().map(|pctxt| {
                                    let mut acc = pctxt.state().display_acc();
                                    let mut pred = pctxt.state().display_pred();
                                    if !self.foldunfold_state_filter.is_empty() {
                                        let filter = &self.foldunfold_state_filter;
                                        acc = acc
                                            .split("\n")
                                            .filter(|x| x.contains(filter))
                                            .map(|x| x.to_string())
                                            .collect::<Vec<_>>()
                                            .join("\n");
                                        pred = pred
                                            .split("\n")
                                            .filter(|x| x.contains(filter))
                                            .map(|x| x.to_string())
                                            .collect::<Vec<_>>()
                                            .join("\n");
                                    }
                                    vec![format!("Acc:\n{}", acc), format!("Pred:\n{}", pred)]
                                })
                            })
                            .unwrap_or_else(|| vec![])
                    })
                },
            );
        }
    }

    fn check_compatible_back_edge(_left: &PathCtxt<'p>, _right: &PathCtxt<'p>) {
        //let left_state = left.state();
        //let right_state = right.state();

        // TODO: re-enable this consistency check, discarding all places for which `.is_simple_place()` is false
        //debug_assert_eq!(left_state.acc(), right_state.acc(), "back edge (acc)");
        //debug_assert_eq!(left_state.pred(), right_state.pred(), "back edge (pred)");
        //debug_assert_eq!(left_state.framing_stack(), right_state.framing_stack(), "back edge (framing)");
    }

    /// Give the initial branch context
    fn initial_context(&mut self) -> Result<PathCtxt<'p>, Self::Error> {
        Ok(self.initial_pctxt.clone())
    }

    /// Replace some statements, mutating the branch context
    fn replace_stmt(
        &mut self,
        stmt_index: usize,
        stmt: &vir::Stmt,
        is_last_before_return: bool,
        pctxt: &mut PathCtxt<'p>,
        curr_block_index: vir::CfgBlockIndex,
        new_cfg: &vir::CfgMethod,
        label: Option<&str>,
    ) -> Result<Vec<vir::Stmt>, Self::Error> {
        debug!("[enter] replace_stmt: ##### {} #####", stmt);

        if let vir::Stmt::ExpireBorrows(ref dag) = stmt {
            let mut stmts = vec![vir::Stmt::Comment(format!("{}", stmt))];
            stmts.extend(self.process_expire_borrows(
                dag,
                pctxt,
                curr_block_index,
                new_cfg,
                label,
            )?);
            return Ok(stmts);
        }

        let mut stmt = stmt.clone();

        // Store state for old expressions
        match stmt {
            vir::Stmt::Label(ref label) => {
                let mut labelled_pctxt = pctxt.clone();
                let labelled_state = labelled_pctxt.mut_state();
                labelled_state.replace_places(|place| place.old(label));
                self.pctxt_at_label
                    .insert(label.to_string(), labelled_pctxt);
            }

            vir::Stmt::PackageMagicWand(vir::Expr::MagicWand(box ref lhs, _, _, _), ..)
            | vir::Stmt::ApplyMagicWand(vir::Expr::MagicWand(box ref lhs, _, _, _), ..) => {
                // TODO: This should be done also for magic wand expressions inside inhale/exhale.
                let label = "lhs".to_string();
                let mut labelled_pctxt = pctxt.clone();
                let labelled_state = labelled_pctxt.mut_state();
                labelled_state.remove_all();
                vir::Stmt::Inhale(lhs.clone(), vir::FoldingBehaviour::Expr)
                    .apply_on_state(labelled_state, pctxt.predicates());
                if let vir::Expr::PredicateAccessPredicate(ref _name, box ref arg, perm_amount, _) =
                    lhs
                {
                    labelled_state.insert_acc(arg.clone(), *perm_amount);
                }
                labelled_state.replace_places(|place| place.old(&label));
                self.pctxt_at_label
                    .insert(label.to_string(), labelled_pctxt);
            }

            _ => {} // Nothing
        }

        let mut stmts: Vec<vir::Stmt> = vec![];

        if stmt_index == 0 && config::dump_path_ctxt_in_debug_info() {
            let acc_state = pctxt.state().display_acc().replace("\n", "\n//");
            stmts.push(vir::Stmt::comment(format!(
                "[state] acc: {{\n//{}\n//}}",
                acc_state
            )));
            let pred_state = pctxt.state().display_pred().replace("\n", "\n//");
            stmts.push(vir::Stmt::comment(format!(
                "[state] pred: {{\n//{}\n//}}",
                pred_state
            )));
            let moved_state = pctxt.state().display_moved().replace("\n", "\n//");
            stmts.push(vir::Stmt::comment(format!(
                "[state] moved: {{\n//{}\n//}}",
                moved_state
            )));
        }

        // 1. Insert "unfolding in" inside old expressions. This handles *old* requirements.
        debug!("[step.1] replace_stmt: {}", stmt);
        stmt = self.rewrite_stmt_with_unfoldings_in_old(stmt, &pctxt)?;

        // 2. Obtain required *curr* permissions. *old* requirements will be handled at steps 0 and/or 4.
        debug!("[step.2] replace_stmt: {}", stmt);
        match &stmt {
            vir::Stmt::Inhale(_, vir::FoldingBehaviour::Expr)
            | vir::Stmt::Assert(_, vir::FoldingBehaviour::Expr, _) => {
                // Unfolding expressions will be added in step 4.
            }
            _ => {
                let all_perms = stmt.get_required_permissions(pctxt.predicates());
                let pred_permissions: Vec<_> =
                    all_perms.iter().cloned().filter(|p| p.is_pred()).collect();

                let acc_permissions: Vec<_> = all_perms
                    .into_iter()
                    .filter(|p| {
                        if !p.is_acc() {
                            false
                        } else {
                            if p.is_curr() {
                                true
                            } else {
                                pred_permissions
                                    .iter()
                                    .any(|pred_p| pred_p.get_place() == p.get_place())
                            }
                        }
                    })
                    .collect();

                let mut perms = acc_permissions;
                perms.extend(pred_permissions.into_iter());
                debug!(
                    "required permissions: {{\n{}\n}}",
                    perms
                        .iter()
                        .map(|x| format!("  {:?}", x))
                        .collect::<Vec<_>>()
                        .join(",\n")
                );

                if !perms.is_empty() {
                    stmts.extend(pctxt.obtain_permissions(perms)?.iter().map(|a| a.to_stmt()));

                    if self.check_foldunfold_state && !is_last_before_return {
                        stmts.push(vir::Stmt::comment("Assert content of fold/unfold state"));
                        stmts.push(vir::Stmt::Assert(
                            pctxt.state().as_vir_expr(),
                            vir::FoldingBehaviour::Expr,
                            vir::Position::default(),
                        ));
                    }
                }
            }
        }

        // 3. Replace special statements
        debug!("[step.3] replace_stmt: {}", stmt);
        stmt = match stmt {
            vir::Stmt::PackageMagicWand(
                vir::Expr::MagicWand(box ref lhs, box ref rhs, _, _),
                ref old_package_stmts,
                ref label,
                ref vars,
                ref position,
            ) => {
                let mut package_pctxt = pctxt.clone();
                let mut package_stmts = vec![];
                for stmt in old_package_stmts {
                    package_stmts.extend(self.replace_stmt(
                        stmt_index,
                        stmt,
                        false,
                        &mut package_pctxt,
                        curr_block_index,
                        new_cfg,
                        Some(label),
                    )?);
                    if config::dump_debug_info() {
                        //package_stmts.push(
                        //    vir::Stmt::comment(format!("[state] acc: {{\n{}\n}}", package_pctxt.state().display_acc()))
                        //);
                        //package_stmts.push(
                        //    vir::Stmt::comment(format!("[state] pred: {{\n{}\n}}", package_pctxt.state().display_pred()))
                        //);
                        report::log::report(
                            "vir_package",
                            "package.vir",
                            vir::Stmt::package_magic_wand(
                                lhs.clone(),
                                rhs.clone(),
                                package_stmts.clone(),
                                label.clone(),
                                vars.clone(),
                                position.clone(),
                            ),
                        );
                    }
                }
                vir::Stmt::package_magic_wand(
                    lhs.clone(),
                    rhs.clone(),
                    package_stmts,
                    label.clone(),
                    vars.clone(),
                    position.clone(),
                )
            }
            stmt => stmt,
        };

        // 4. Add "unfolding" expressions in statement. This handles *old* requirements.
        debug!("[step.4] replace_stmt: Add unfoldings in stmt {}", stmt);
        stmt = self.rewrite_stmt_with_unfoldings(stmt, &pctxt)?;

        // 5. Apply effect of statement on state
        debug!("[step.5] replace_stmt: {}", stmt);
        stmt = match stmt {
            vir::Stmt::If(cond, then_stmts, else_stmts) => {
                let mut then_pctxt = pctxt.clone();
                let mut then_stmts = then_stmts.into_iter()
                    .map(|stmt| self.replace_stmt(
                        stmt_index, &stmt, false, &mut then_pctxt, curr_block_index, new_cfg, None
                    ))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter().flatten()
                    .collect::<Vec<_>>();
                let mut else_pctxt = pctxt.clone();
                let mut else_stmts = else_stmts.into_iter()
                    .map(|stmt| self.replace_stmt(
                        stmt_index, &stmt, false, &mut else_pctxt, curr_block_index, new_cfg, None
                    ))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter().flatten()
                    .collect::<Vec<_>>();
                let (mut join_actions, mut joined_pctxt) = self.prepend_join(
                    vec![&then_pctxt, &else_pctxt])?;
                else_stmts.extend(self.perform_prejoin_action(
                    &mut joined_pctxt, curr_block_index, join_actions.remove(1))?);
                then_stmts.extend(self.perform_prejoin_action(
                    &mut joined_pctxt, curr_block_index, join_actions.remove(0))?);
                *pctxt = joined_pctxt;
                vir::Stmt::If(cond, then_stmts, else_stmts)
            },
            _ => {
                pctxt.apply_stmt(&stmt);
                stmt
            }
        };
        stmts.push(stmt.clone());

        // 6. Recombine permissions into full if read was carved out during fold.
        if let vir::Stmt::Inhale(expr, vir::FoldingBehaviour::Stmt) = &stmt {
            // We may need to recombine predicates for which read permission was taking during
            // an unfold operation.
            let inhaled_places = expr.extract_predicate_places(vir::PermAmount::Read);
            let restorable_places: Vec<_> = pctxt
                .state()
                .pred()
                .iter()
                .filter(|(place, perm)| {
                    **perm == vir::PermAmount::Remaining
                        && inhaled_places.iter().any(|ip| place.has_prefix(ip))
                })
                .map(|(place, _)| place.clone())
                .collect();
            for place in restorable_places {
                let stmt = vir::Stmt::Obtain(
                    vir::Expr::pred_permission(place, vir::PermAmount::Read).unwrap(),
                    vir::Position::default(), // This should trigger only unfolds,
                                              // so the default position should be fine.
                );
                stmts.extend(self.replace_stmt(
                    stmt_index,
                    &stmt,
                    false,
                    pctxt,
                    curr_block_index,
                    new_cfg,
                    label,
                )?);
            }
        }

        // 7. Handle shared borrows.
        debug!("[step.6] replace_stmt: {}", stmt);
        if let vir::Stmt::Assign(
            ref lhs_place,
            ref rhs_place,
            vir::AssignKind::SharedBorrow(borrow),
        ) = stmt
        {
            // Check if in the state we have any write permissions
            // with the borrowed place as a prefix. If yes, change them
            // to read permissions and emit exhale acc(T(place), write-read).
            let mut acc_perm_counter = 0;
            let mut acc_perms: Vec<_> = pctxt
                .state()
                .acc()
                .iter()
                .filter(|&(place, perm_amount)| {
                    assert!(perm_amount.is_valid_for_specs());
                    place.has_prefix(rhs_place) && !place.is_local()
                })
                .map(|(place, perm_amount)| {
                    acc_perm_counter += 1;
                    (place.clone(), perm_amount.clone(), acc_perm_counter)
                })
                .collect();
            acc_perms.sort_by(|(place1, _, id1), (place2, _, id2)| {
                let key1 = (place1.place_depth(), id1);
                let key2 = (place2.place_depth(), id2);
                key1.cmp(&key2)
            });
            trace!(
                "acc_perms = {}",
                acc_perms
                    .iter()
                    .map(|(a, p, id)| format!("({}, {}, {}), ", a, p, id))
                    .collect::<String>()
            );
            for (place, perm_amount, _) in acc_perms {
                debug!("acc place: {} {}", place, perm_amount);
                debug!(
                    "rhs_place={} {:?}",
                    rhs_place,
                    pctxt.state().acc().get(rhs_place)
                );
                debug!(
                    "lhs_place={} {:?}",
                    lhs_place,
                    pctxt.state().acc().get(lhs_place)
                );
                if *rhs_place == place {
                    if Some(&vir::PermAmount::Write) == pctxt.state().acc().get(lhs_place) {
                        // We are copying a shared reference, so we do not need to change
                        // the root of rhs.
                        debug!("Copy of a shared reference. Ignore.");
                        continue;
                    }
                }
                if perm_amount == vir::PermAmount::Write {
                    let access = vir::Expr::FieldAccessPredicate(
                        box place.clone(),
                        vir::PermAmount::Remaining,
                        vir::Position::default(),
                    );
                    pctxt
                        .log_mut()
                        .log_convertion_to_read(borrow, access.clone());
                    let stmt = vir::Stmt::Exhale(access, self.method_pos.clone());
                    pctxt.apply_stmt(&stmt);
                    stmts.push(stmt);
                }
                let new_place = place.replace_place(rhs_place, lhs_place);
                debug!("    new place: {}", new_place);
                let lhs_read_access = vir::Expr::FieldAccessPredicate(
                    box new_place,
                    vir::PermAmount::Read,
                    vir::Position::default(),
                );
                pctxt.log_mut().log_read_permission_duplication(
                    borrow,
                    lhs_read_access.clone(),
                    lhs_place.clone(),
                );
                let stmt = vir::Stmt::Inhale(lhs_read_access, vir::FoldingBehaviour::Stmt);
                pctxt.apply_stmt(&stmt);
                stmts.push(stmt);
            }
            let pred_perms: Vec<_> = pctxt
                .state()
                .pred()
                .iter()
                .filter(|&(place, perm_amount)| {
                    assert!(perm_amount.is_valid_for_specs());
                    place.has_prefix(rhs_place)
                })
                .map(|(place, perm_amount)| (place.clone(), perm_amount.clone()))
                .collect();
            for (place, perm_amount) in pred_perms {
                debug!("pred place: {} {}", place, perm_amount);
                let predicate_name = place.typed_ref_name().unwrap();
                if perm_amount == vir::PermAmount::Write {
                    let access = vir::Expr::PredicateAccessPredicate(
                        predicate_name.clone(),
                        box place.clone(),
                        vir::PermAmount::Remaining,
                        place.pos(),
                    );
                    pctxt
                        .log_mut()
                        .log_convertion_to_read(borrow, access.clone());
                    let stmt = vir::Stmt::Exhale(access, self.method_pos);
                    pctxt.apply_stmt(&stmt);
                    stmts.push(stmt);
                }
                let new_place = place.replace_place(rhs_place, lhs_place);
                debug!("    new place: {}", new_place);
                let lhs_read_access = vir::Expr::PredicateAccessPredicate(
                    predicate_name,
                    box new_place,
                    vir::PermAmount::Read,
                    vir::Position::default(),
                );
                pctxt.log_mut().log_read_permission_duplication(
                    borrow,
                    lhs_read_access.clone(),
                    lhs_place.clone(),
                );
                let stmt = vir::Stmt::Inhale(lhs_read_access, vir::FoldingBehaviour::Stmt);
                pctxt.apply_stmt(&stmt);
                stmts.push(stmt);
            }
        }

        // Delete lhs state
        // TODO: Stack of LHS pctxts and pop the top one if we exit a magic wand?
        // self.pctxt_at_label.remove("lhs");

        debug!(
            "[exit] replace_stmt = [\n{}\n]",
            stmts
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join("\n  ")
        );
        Ok(stmts)
    }

    /// Inject some statements and replace a successor, mutating the branch context
    fn replace_successor(
        &mut self,
        succ: &vir::Successor,
        pctxt: &mut PathCtxt<'p>,
    ) -> Result<(Vec<vir::Stmt>, vir::Successor), Self::Error> {
        debug!("replace_successor: {}", succ);
        let exprs: Vec<&vir::Expr> = match succ {
            &vir::Successor::GotoSwitch(ref guarded_targets, _) => {
                guarded_targets.iter().map(|g| &g.0).collect()
            }
            _ => vec![],
        };

        let grouped_perms: HashMap<_, _> = exprs
            .iter()
            .flat_map(|e| e.get_required_permissions(pctxt.predicates()))
            .group_by_label();

        let mut stmts: Vec<vir::Stmt> = vec![];

        let mut some_perms_required = false;
        for (label, perms) in grouped_perms.into_iter() {
            debug!("Obtain at label {:?} permissions {:?}", label, perms);
            // Hack: skip old permissions
            if label.is_some() {
                continue;
            }
            if !perms.is_empty() {
                some_perms_required = true;
                let mut opt_old_pctxt =
                    label.map(|label_name| self.pctxt_at_label.get(&label_name).unwrap().clone());
                let label_pctxt = opt_old_pctxt.as_mut().unwrap_or(pctxt);
                stmts.extend(
                    label_pctxt
                        .obtain_permissions(perms)?
                        .iter()
                        .map(|a| a.to_stmt())
                        .collect::<Vec<_>>(),
                );
            }
        }

        if some_perms_required && self.check_foldunfold_state {
            stmts.push(vir::Stmt::comment("Assert content of fold/unfold state"));
            stmts.push(vir::Stmt::Assert(
                pctxt.state().as_vir_expr(),
                vir::FoldingBehaviour::Expr,
                vir::Position::default(),
            ));
        }

        // Add "fold/unfolding in" expressions in successor
        let repl_expr = |expr: &vir::Expr| -> Result<vir::Expr, FoldUnfoldError> {
            self.replace_expr(expr, pctxt)
        };

        let new_succ = match succ {
            vir::Successor::Undefined => vir::Successor::Undefined,
            vir::Successor::Return => vir::Successor::Return,
            vir::Successor::Goto(target) => vir::Successor::Goto(*target),
            vir::Successor::GotoSwitch(guarded_targets, default_target) => {
                vir::Successor::GotoSwitch(
                    guarded_targets
                        .iter()
                        .map(|(cond, targ)| repl_expr(cond).map(|expr| (expr, targ.clone())))
                        .collect::<Result<Vec<_>, _>>()?,
                    *default_target,
                )
            }
        };

        Ok((stmts, new_succ))
    }

    /// Compute actions that need to be performed before the join point,
    /// returning the merged branch context.
    fn prepend_join(
        &mut self,
        bcs: Vec<&PathCtxt<'p>>,
    ) -> Result<(Vec<ActionVec>, PathCtxt<'p>), Self::Error> {
        trace!("[enter] prepend_join(..{})", &bcs.len());
        assert!(bcs.len() > 0);
        if bcs.len() == 1 {
            Ok((vec![ActionVec(vec![])], bcs[0].clone()))
        } else {
            // Define two subgroups
            let mid = bcs.len() / 2;
            let left_pctxts = &bcs[..mid];
            let right_pctxts = &bcs[mid..];

            // Join the subgroups
            let (left_actions_vec, mut left_pctxt) = self.prepend_join(left_pctxts.to_vec())?;
            let (right_actions_vec, right_pctxt) = self.prepend_join(right_pctxts.to_vec())?;

            // Join the recursive calls
            let (merge_actions_left, merge_actions_right) = left_pctxt.join(right_pctxt)?;
            let merged_pctxt = left_pctxt;

            let mut branch_actions_vec: Vec<ActionVec> = vec![];
            for mut left_actions in left_actions_vec {
                left_actions.0.extend(merge_actions_left.iter().cloned());
                branch_actions_vec.push(left_actions);
            }
            for mut right_actions in right_actions_vec {
                right_actions.0.extend(merge_actions_right.iter().cloned());
                branch_actions_vec.push(right_actions);
            }

            trace!(
                "[exit] prepend_join(..{}): {}",
                &bcs.len(),
                branch_actions_vec
                    .iter()
                    .map(|v| format!("[{}]", v.iter().to_sorted_multiline_string()))
                    .to_string()
            );
            Ok((branch_actions_vec, merged_pctxt))
        }
    }

    /// Convert actions to statements and log them.
    fn perform_prejoin_action(
        &mut self,
        pctxt: &mut PathCtxt,
        block_index: CfgBlockIndex,
        actions: ActionVec,
    ) -> Result<Vec<vir::Stmt>, Self::Error> {
        let mut stmts = Vec::new();
        for action in actions.0 {
            stmts.push(action.to_stmt());
            pctxt.log_mut().log_prejoin_action(block_index, action);
        }
        Ok(stmts)
    }
}

struct ExprReplacer<'b, 'a: 'b> {
    curr_pctxt: PathCtxt<'a>,
    pctxt_at_label: &'b HashMap<String, PathCtxt<'a>>,
    lhs_pctxt: Option<PathCtxt<'a>>,
    wait_old_expr: bool,
}

impl<'b, 'a: 'b> ExprReplacer<'b, 'a> {
    pub fn new(
        curr_pctxt: PathCtxt<'a>,
        pctxt_at_label: &'b HashMap<String, PathCtxt<'a>>,
        wait_old_expr: bool,
    ) -> Self {
        ExprReplacer {
            curr_pctxt,
            pctxt_at_label,
            lhs_pctxt: None,
            wait_old_expr,
        }
    }
}

impl<'b, 'a: 'b> FallibleExprFolder for ExprReplacer<'b, 'a> {
    type Error = FoldUnfoldError;

    fn fallible_fold_field(
        &mut self,
        expr: Box<vir::Expr>,
        field: vir::Field,
        pos: vir::Position,
    ) -> Result<vir::Expr, Self::Error> {
        debug!("[enter] fold_field {}, {}", expr, field);

        let res = if self.wait_old_expr {
            vir::Expr::Field(self.fallible_fold_boxed(expr)?, field, pos)
        } else {
            // FIXME: we lose positions
            let (base, mut components) = expr.explode_place();
            components.push(vir::PlaceComponent::Field(field, pos));
            let new_base = self.fallible_fold(base)?;
            debug_assert!(
                match new_base {
                    vir::Expr::Local(..) | vir::Expr::LabelledOld(..) => true,
                    _ => false,
                },
                "new_base = {}",
                new_base
            );
            new_base.reconstruct_place(components)
        };

        debug!("[exit] fold_unfolding = {}", res);
        Ok(res)
    }

    fn fallible_fold_unfolding(
        &mut self,
        name: String,
        args: Vec<vir::Expr>,
        expr: Box<vir::Expr>,
        perm: vir::PermAmount,
        variant: vir::MaybeEnumVariantIndex,
        pos: vir::Position,
    ) -> Result<vir::Expr, Self::Error> {
        debug!(
            "[enter] fold_unfolding {}, {}, {}, {}",
            name, args[0], expr, perm
        );

        let res = if self.wait_old_expr {
            vir::Expr::Unfolding(
                name,
                args,
                self.fallible_fold_boxed(expr)?,
                perm,
                variant,
                pos,
            )
        } else {
            // Compute inner state
            let mut inner_pctxt = self.curr_pctxt.clone();
            let inner_state = inner_pctxt.mut_state();
            vir::Stmt::Unfold(name.clone(), args.clone(), perm, variant.clone())
                .apply_on_state(inner_state, self.curr_pctxt.predicates());

            // Store states
            let mut tmp_curr_pctxt = inner_pctxt;
            std::mem::swap(&mut self.curr_pctxt, &mut tmp_curr_pctxt);

            let inner_expr = self.fallible_fold_boxed(expr)?;

            // Restore states
            std::mem::swap(&mut self.curr_pctxt, &mut tmp_curr_pctxt);

            vir::Expr::Unfolding(name, args, inner_expr, perm, variant, pos)
        };

        debug!("[exit] fold_unfolding = {}", res);
        Ok(res)
    }

    fn fallible_fold_magic_wand(
        &mut self,
        lhs: Box<vir::Expr>,
        rhs: Box<vir::Expr>,
        borrow: Option<vir::borrows::Borrow>,
        pos: vir::Position,
    ) -> Result<vir::Expr, Self::Error> {
        debug!("[enter] fold_magic_wand {}, {}", lhs, rhs);

        // Compute lhs state
        let mut lhs_pctxt = self.curr_pctxt.clone();
        let lhs_state = lhs_pctxt.mut_state();
        lhs_state.remove_all();
        lhs_state.insert_all_perms(
            lhs.get_permissions(self.curr_pctxt.predicates())
                .into_iter()
                .filter(|p| p.is_pred())
                .flat_map(|p| vec![Perm::acc(p.get_place().clone(), p.get_perm_amount()), p]),
        );
        lhs_state.replace_places(|place| {
            let pos = place.pos();
            place.old("lhs").set_pos(pos)
        });
        debug!("State of lhs of magic wand: {}", lhs_state);

        // Store states
        std::mem::swap(&mut self.curr_pctxt, &mut lhs_pctxt);

        // Rewrite lhs
        let new_lhs = self.fallible_fold_boxed(lhs)?;

        // Restore states
        std::mem::swap(&mut self.curr_pctxt, &mut lhs_pctxt);

        // Computer lhs state, now with unfoldings
        let mut new_lhs_pctxt = self.curr_pctxt.clone();
        let new_lhs_state = new_lhs_pctxt.mut_state();
        new_lhs_state.remove_all();
        new_lhs_state.insert_all_perms(
            new_lhs
                .get_permissions(self.curr_pctxt.predicates())
                .into_iter()
                .filter(|p| p.is_pred())
                .flat_map(|p| vec![Perm::acc(p.get_place().clone(), p.get_perm_amount()), p]),
        );
        new_lhs_state.replace_places(|place| {
            let pos = place.pos();
            place.old("lhs").set_pos(pos)
        });
        debug!("New state of lhs of magic wand: {}", new_lhs_state);

        // Compute rhs state
        let mut rhs_pctxt = self.curr_pctxt.clone();
        let rhs_state = rhs_pctxt.mut_state();
        rhs_state.remove_all();
        rhs_state.insert_all_perms(
            rhs.get_permissions(self.curr_pctxt.predicates())
                .into_iter()
                .filter(|p| p.is_pred())
                .flat_map(|p| vec![Perm::acc(p.get_place().clone(), p.get_perm_amount()), p]),
        );
        debug!("State of rhs of magic wand: {}", rhs_state);

        // Store states
        std::mem::swap(&mut self.curr_pctxt, &mut rhs_pctxt);
        self.lhs_pctxt = Some(new_lhs_pctxt);

        // Rewrite rhs
        let new_rhs = self.fallible_fold_boxed(rhs)?;

        // Restore states
        self.lhs_pctxt = None;
        std::mem::swap(&mut self.curr_pctxt, &mut rhs_pctxt);

        // Rewrite lhs and build magic wand
        let res = vir::Expr::MagicWand(new_lhs, new_rhs, borrow, pos);

        debug!("[enter] fold_magic_wand = {}", res);
        Ok(res)
    }

    fn fallible_fold_labelled_old(
        &mut self,
        label: String,
        expr: Box<vir::Expr>,
        pos: vir::Position,
    ) -> Result<vir::Expr, Self::Error> {
        debug!("[enter] fold_labelled_old {}: {}", label, expr);

        let mut tmp_curr_pctxt = if label == "lhs" && self.lhs_pctxt.is_some() {
            self.lhs_pctxt.as_ref().unwrap().clone()
        } else {
            self.pctxt_at_label.get(&label).unwrap().clone()
        };

        // Replace old[label] with curr
        tmp_curr_pctxt.mut_state().replace_places(|place| {
            place.map_labels(|opt_label| {
                if opt_label == label.clone() {
                    None
                } else {
                    Some(opt_label)
                }
            })
        });

        // Store states
        std::mem::swap(&mut self.curr_pctxt, &mut tmp_curr_pctxt);
        let old_wait_old_expr = self.wait_old_expr;
        self.wait_old_expr = false;

        // Rewrite inner expression
        let inner_expr = self.fallible_fold_boxed(expr)?;

        // Restore states
        std::mem::swap(&mut self.curr_pctxt, &mut tmp_curr_pctxt);
        self.wait_old_expr = old_wait_old_expr;

        // Rebuild expression
        let res = vir::Expr::LabelledOld(label, inner_expr, pos);

        debug!("[exit] fold_labelled_old = {}", res);
        Ok(res)
    }

    fn fallible_fold(&mut self, expr: vir::Expr) -> Result<vir::Expr, Self::Error> {
        debug!("[enter] fold {}", expr);

        let res = if self.wait_old_expr || !expr.is_pure() {
            vir::default_fallible_fold_expr(self, expr)?
        } else {
            // Try to add unfolding
            let inner_expr = vir::default_fallible_fold_expr(self, expr)?;
            let perms: Vec<_> = inner_expr
                .get_required_permissions(self.curr_pctxt.predicates())
                .into_iter()
                .filter(|p| p.is_curr())
                .collect();

            debug!(
                "get_required_permissions for {}: {{\n  {}\n}}",
                inner_expr,
                perms
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(",\n  ")
            );

            // Add appropriate unfolding around this old expression
            // Note: unfoldings must have no effect on siblings
            let mut result = inner_expr;
            for action in self
                .curr_pctxt
                .clone()
                .obtain_permissions(perms)?
                .into_iter()
                .rev()
            {
                result = action.to_expr(result)?;
            }
            result
        };

        debug!("[exit] fold = {}", res);
        Ok(res)
    }

    fn fallible_fold_func_app(
        &mut self,
        function_name: String,
        args: Vec<vir::Expr>,
        formal_args: Vec<vir::LocalVar>,
        return_type: vir::Type,
        position: vir::Position,
    ) -> Result<vir::Expr, Self::Error> {
        if self.wait_old_expr {
            Ok(vir::Expr::FuncApp(
                function_name,
                args.into_iter()
                    .map(|e| self.fallible_fold(e))
                    .collect::<Result<Vec<_>, Self::Error>>()?,
                formal_args.clone(),
                return_type.clone(),
                position.clone(),
            ))
        } else {
            let func_app =
                vir::Expr::FuncApp(function_name, args, formal_args, return_type, position);

            trace!("[enter] fold_func_app {}", func_app);

            let perms: Vec<_> = func_app
                .get_required_permissions(self.curr_pctxt.predicates())
                .into_iter()
                .collect();

            let mut result = func_app;
            for action in self
                .curr_pctxt
                .clone()
                .obtain_permissions(perms)?
                .into_iter()
                .rev()
            {
                result = action.to_expr(result)?;
            }

            trace!("[exit] fold_func_app {}", result);
            Ok(result)
        }
    }
}
