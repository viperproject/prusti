// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::legacy::{self, CfgMethod, CfgBlock, CfgBlockIndex};
use std::collections::HashSet;

pub fn collect_assigned_vars(
    method: &CfgMethod,
    start_block: CfgBlockIndex,
    end_block: CfgBlockIndex,
) -> HashSet<legacy::LocalVar> {
    let predecessors = method.predecessors();
    let start = start_block.block_index;
    let end = end_block.block_index;
    let mut result = HashSet::new();
    let mut marked = HashSet::new();
    marked.insert(end);
    marked.insert(start);
    let mut to_visit = vec![start];
    while let Some(current) = to_visit.pop() {
        if let Some(current_predecessors) = predecessors.get(&current) {
            for predecessor in current_predecessors {
                if !marked.contains(predecessor) {
                    to_visit.push(*predecessor);
                    marked.insert(*predecessor);
                }
            }
        }
        check_block(&mut result, &method.basic_blocks[current]);
    }
    result
}

fn check_block(
    vars: &mut HashSet<legacy::LocalVar>,
    block: &CfgBlock
) {
    for stmt in &block.stmts {
        match stmt {
            legacy::Stmt::MethodCall(_, _, targets) => {
                vars.extend(targets.iter().cloned());
            }
            legacy::Stmt::Assign(legacy::Expr::Local(var, _), _, _) => {
                vars.insert(var.clone());
            }
            _ => {}
        }
    }
}
