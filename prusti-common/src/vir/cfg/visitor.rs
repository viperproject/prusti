// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::vir::{
    ast::*,
    cfg::method::*,
};
use crate::utils::to_string::ToString;
use std::fmt::Debug;

pub trait CheckNoOpAction {
    /// Is the action a no operation?
    fn is_noop(&self) -> bool;
}

/// Visit the reachable blocks of a CFG with a forward pass.
/// During the visit, statements can be modified and injected.
/// However, the structure of the CFG can not change.
/// For each branch a context is updated, duplicated at forks, and merged with other contexts at joins.
pub trait CfgReplacer<
    PathCtxt: Debug + Clone,
    Action: CheckNoOpAction + Debug
> {
    type Error;

    /*
    /// Define `lub` by using the definition of `join`
    fn contained_in(&mut self, left: &PathCtxt, right: &PathCtxt) -> bool {
        let (_, joined) = self.prepend_join(vec![left, right]);
        left == right
    }
    */

    /// Callback method called each time the CFG is modified. Useful for debugging purposes.
    fn current_cfg(
        &self,
        _cfg: &CfgMethod,
        _initial_pctxt: &[Option<PathCtxt>],
        _final_pctxt: &[Option<PathCtxt>],
    ) {}

    /// Are two branch context compatible for a back edge?
    fn check_compatible_back_edge(left: &PathCtxt, right: &PathCtxt);

    /// Give the initial branch context
    fn initial_context(&mut self) -> Result<PathCtxt, Self::Error>;

    /// Replace some statements, mutating the branch context
    ///
    /// ``label`` is the label to be used for references when emitting
    /// fold/unfold operations. This is a hacky work-around for Silicon
    /// not being able to see that two locations are the same inside the
    /// package statement. The minimal example that illustrates the issue:
    ///
    /// ```rust,ignore
    /// pub struct ListNode {
    ///     next: Option<Box<ListNode>>,
    /// }
    /// pub fn test5(list: &mut ListNode) -> &mut ListNode {
    ///     match list.next {
    ///         Some(box ref mut node) => test5(node),
    ///         None => list,
    ///     }
    /// }
    /// ```
    fn replace_stmt(
        &mut self,
        stmt_index: usize,
        stmt: &Stmt,
        is_last_before_return: bool,
        pctxt: &mut PathCtxt,
        curr_block_index: CfgBlockIndex,
        new_cfg: &CfgMethod,
        label: Option<&str>,
    ) -> Result<Vec<Stmt>, Self::Error>;

    /// Inject some statements and replace a successor, mutating the branch context
    fn replace_successor(
        &mut self,
        succ: &Successor,
        pctxt: &mut PathCtxt,
    ) -> Result<(Vec<Stmt>, Successor), Self::Error>;

    /// Compute actions that need to be performed before the join point,
    /// returning the merged branch context.
    fn prepend_join(
        &mut self,
        pctxts: Vec<&PathCtxt>
    ) -> Result<(Vec<Action>, PathCtxt), Self::Error>;

    /// Convert actions to statements.
    fn perform_prejoin_action(
        &mut self,
        pctxt: &mut PathCtxt,
        block_index: CfgBlockIndex,
        actions: Action) -> Result<Vec<Stmt>, Self::Error>;

    /// The main method: visit and replace the reachable blocks of a CFG.
    fn replace_cfg(&mut self, cfg: &CfgMethod) -> Result<CfgMethod, Self::Error> {
        // Initialize the variables of the new cfg
        let mut new_cfg = CfgMethod::new(
            cfg.method_name.clone(),
            cfg.formal_arg_count,
            cfg.formal_returns.clone(),
            cfg.local_vars.clone(),
            cfg.get_all_labels(),
            cfg.var_debug_info.clone(),
        );

        // Initialize the blocks of the new cfg
        for (index, _block) in cfg.basic_blocks.iter().enumerate() {
            let label = &cfg.basic_blocks_labels[index];
            new_cfg.add_block(label, vec![]);
        }

        // Find the blocks
        //
        let to_visit: Vec<usize> = cfg
            .get_topological_sort()
            .iter()
            .map(|x| x.block_index)
            .collect();
        let mut visited: Vec<bool> = vec![false; cfg.basic_blocks.len()];
        let mut reachable: Vec<bool> = vec![false; cfg.basic_blocks.len()];
        let mut initial_pctxt: Vec<Option<PathCtxt>> = vec![None; cfg.basic_blocks.len()];
        let mut final_pctxt: Vec<Option<PathCtxt>> = vec![None; cfg.basic_blocks.len()];
        reachable[0] = true;

        for curr_index in to_visit {
            assert!(!visited[curr_index]);
            visited[curr_index] = true;

            let curr_block_index = new_cfg.block_index(curr_index);
            let curr_block = &cfg.basic_blocks[curr_index];

            // Skip current block if unreachable
            if !reachable[curr_index] {
                debug!("Skip unreachable block {:?}", curr_block_index);
                // Set statements and successor of unreachable blocks
                // TODO: delete the blocks instead
                for stmt in &curr_block.stmts {
                    new_cfg.add_stmt(curr_block_index, stmt.clone());
                }
                new_cfg.set_successor(
                    curr_block_index,
                    curr_block.successor.clone().replace_uuid(new_cfg.uuid),
                );
                continue;
            }

            // Mark following blocks as reachable
            for following_block in curr_block.successor.get_following() {
                reachable[following_block.block_index] = true;
            }

            // JOIN incoming visited edges. This may add one basic block for each incoming edge.
            debug!(
                "Incoming blocks to {:?}: {:?}",
                curr_block_index,
                cfg.get_preceding(cfg.block_index(curr_index))
            );
            let incoming_edges: Vec<usize> = cfg
                .get_preceding(cfg.block_index(curr_index))
                .into_iter()
                .map(|x| x.block_index)
                .filter(|i| visited[*i])
                .collect();
            debug!(
                "Incoming visited blocks to {:?}: {:?}",
                curr_block_index, &incoming_edges
            );
            let incoming_pctxt: Vec<&PathCtxt> = incoming_edges
                .iter()
                .map(|i| final_pctxt[*i].as_ref().unwrap())
                .collect();
            let mut pctxt: PathCtxt;
            if incoming_pctxt.is_empty() {
                pctxt = self.initial_context()?;
            } else {
                let actions_and_pctxt = self.prepend_join(incoming_pctxt)?;
                let actions = actions_and_pctxt.0;
                pctxt = actions_and_pctxt.1;
                for (&src_index, action) in incoming_edges.iter().zip(actions) {
                    assert!(visited[src_index]);
                    if !action.is_noop() {
                        let src_block_index = new_cfg.block_index(src_index);
                        trace!(
                            "Perform action to {:?} coming from {:?}: {:?}",
                            curr_block_index,
                            src_block_index,
                            action
                        );
                        let new_label = new_cfg.get_fresh_label_name();
                        let new_block_index = new_cfg.add_block(
                            &new_label,
                            vec![Stmt::comment(format!(
                                "========== {} ==========",
                                new_label
                            ))],
                        );
                        let stmts_to_add = self.perform_prejoin_action(&mut pctxt, new_block_index, action)?;
                        new_cfg.add_stmts(new_block_index, stmts_to_add);
                        new_cfg.set_successor(new_block_index, Successor::Goto(curr_block_index));
                        new_cfg.set_successor(
                            src_block_index,
                            new_cfg.basic_blocks[src_index]
                                .successor
                                .clone()
                                .replace_target(curr_block_index, new_block_index),
                        );
                    }
                }
            }

            // Store initial pctxt
            trace!("Initial pctxt of {:?}: {:?}", curr_block_index, &pctxt);
            initial_pctxt[curr_index] = Some(pctxt.clone());

            // REPLACE statement
            for (stmt_index, stmt) in curr_block.stmts.iter().enumerate() {
                self.current_cfg(&new_cfg, &initial_pctxt, &final_pctxt);
                let last_stmt_before_return =
                    stmt_index == curr_block.stmts.len() - 1 && curr_block.successor.is_return();
                let new_stmts = self.replace_stmt(
                    stmt_index,
                    stmt,
                    last_stmt_before_return,
                    &mut pctxt,
                    curr_block_index,
                    &new_cfg,
                    None,
                )?;
                trace!(
                    "Replace stmt '{}' with [{}]",
                    stmt,
                    new_stmts.iter().to_string()
                );
                for new_stmt in new_stmts {
                    new_cfg.add_stmt(curr_block_index, new_stmt);
                }
            }

            // REPLACE successor
            self.current_cfg(&new_cfg, &initial_pctxt, &final_pctxt);
            let (new_stmts, new_successor) =
                self.replace_successor(&curr_block.successor, &mut pctxt)?;
            trace!(
                "Replace successor of {:?} with {:?} and {:?}",
                curr_block_index,
                new_stmts,
                new_successor
            );
            for new_stmt in new_stmts {
                new_cfg.add_stmt(curr_block_index, new_stmt);
            }
            // Check that the structure of the CFG is unchanged
            assert_eq!(
                curr_block
                    .successor
                    .get_following()
                    .iter()
                    .map(|x| x.block_index)
                    .collect::<Vec<_>>(),
                new_successor
                    .get_following()
                    .iter()
                    .map(|x| x.block_index)
                    .collect::<Vec<_>>(),
                "The structure of the CFG changed"
            );
            // Add successor
            new_cfg.set_successor(curr_block_index, new_successor.replace_uuid(new_cfg.uuid));

            // Check if there is any back edge
            let following = new_cfg.basic_blocks[curr_index].successor.get_following();
            for following_index in following {
                let index = following_index.block_index;
                if visited[index] {
                    debug!(
                        "Back edge from {:?} to {:?}",
                        curr_block_index, following_index
                    );
                    let other_pctxt = initial_pctxt[index].as_ref().unwrap();
                    Self::check_compatible_back_edge(&pctxt, other_pctxt);
                }
            }

            // Store final pctxt
            trace!("Final pctxt of {:?}: {:?}", curr_block_index, &pctxt);
            final_pctxt[curr_index] = Some(pctxt);
        }

        self.current_cfg(&new_cfg, &initial_pctxt, &final_pctxt);
        Ok(new_cfg)
    }
}

pub trait SuccessorFolder {
    fn fold(&mut self, s: Successor) -> Successor {
        match s {
            Successor::Undefined => self.fold_undefined(),
            Successor::Return => self.fold_return(),
            Successor::Goto(target) => self.fold_goto(target),
            Successor::GotoSwitch(guarded_targets, default_target) => {
                self.fold_goto_switch(guarded_targets, default_target)
            }
        }
    }

    fn fold_expr(&mut self, expr: Expr) -> Expr {
        expr
    }

    fn fold_target(&mut self, target: CfgBlockIndex) -> CfgBlockIndex {
        target
    }

    fn fold_undefined(&mut self) -> Successor {
        Successor::Undefined
    }

    fn fold_return(&mut self) -> Successor {
        Successor::Undefined
    }

    fn fold_goto(&mut self, target: CfgBlockIndex) -> Successor {
        Successor::Goto(self.fold_target(target))
    }

    fn fold_goto_switch(
        &mut self,
        guarded_targets: Vec<(Expr, CfgBlockIndex)>,
        default_target: CfgBlockIndex,
    ) -> Successor {
        Successor::GotoSwitch(
            guarded_targets
                .into_iter()
                .map(|(cond, targ)| (self.fold_expr(cond), targ))
                .collect(),
            self.fold_target(default_target),
        )
    }
}

impl CfgMethod {
    pub fn walk_statements<F: FnMut(&Stmt)>(&self, mut walker: F) {
        for block in self.basic_blocks.iter() {
            for stmt in block.stmts.iter() {
                walker(stmt);
            }
        }
    }

    // TODO: maybe just let basic_blocks be public and modify those directly?
    pub fn patch_statements<E, F: FnMut(Stmt)->Result<Stmt, E>>(
        mut self,
        mut walker: F
    ) -> Result<Self, E> {
        for block in self.basic_blocks.iter_mut() {
            block.stmts = block.stmts.drain(..)
                .map(&mut walker)
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(self)
    }

    pub fn walk_successors<F: FnMut(&Successor)>(&self, mut walker: F) {
        for block in self.basic_blocks.iter() {
            walker(&block.successor);
        }
    }

    /// Visit each expression used in a statement or successor.
    /// Note: sub-expressions of expressions will not be visited.
    pub fn walk_expressions<F: FnMut(&Expr)>(&self, mut walker: F) {
        struct ExprStmtWalker<'a, T: FnMut(&Expr)> {
            walker: &'a mut T,
        }
        impl<'a, T: FnMut(&Expr)> StmtWalker for ExprStmtWalker<'a, T> {
            fn walk_expr(&mut self, expr: &Expr) {
                (self.walker)(expr);
            }
        }
        let mut stmt_walker = ExprStmtWalker{
            walker: &mut walker,
        };
        self.walk_statements(|stmt| stmt_walker.walk(stmt));
        self.walk_successors(|succ| {
            if let Successor::GotoSwitch(targets, _) = succ {
                for (guard, _) in targets {
                    walker(guard);
                }
            }
        });
    }

    /// Remove all statements `s` such that `f(&s)` returns `false`
    pub fn retain_stmts<F: Fn(&Stmt) -> bool>(&mut self, f: F) {
        for block in &mut self.basic_blocks {
            block.stmts.retain(|stmt| f(stmt))
        }
    }
}
