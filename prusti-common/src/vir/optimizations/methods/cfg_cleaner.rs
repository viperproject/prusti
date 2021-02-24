use std::collections::HashMap;

use crate::vir::{self, cfg};

/// Merge consequitive basic blocks that are joined by only one edge.
pub fn clean_cfg(mut method: cfg::CfgMethod) -> cfg::CfgMethod {
    let predecessors = method.predecessors();
    let traversal_order = method.get_topological_sort();
    assert_eq!(traversal_order[0].block_index, 0, "The start block should be first.");
    let mut new_basic_blocks = Vec::new();
    let mut basic_blocks: HashMap<_, _> = method.basic_blocks.into_iter().enumerate().collect();
    let mut new_indices = HashMap::new();

    for cfg::CfgBlockIndex{ block_index, .. } in traversal_order {
        if let Some(mut basic_block) = basic_blocks.remove(&block_index) {
            let new_index = new_basic_blocks.len();
            assert!(new_indices.insert(block_index, new_index).is_none());
            while let vir::cfg::Successor::Goto(target) = &basic_block.successor {
                // If the current basic block has only one successor.
                if predecessors[&target.block_index].len() == 1 {
                    // If the successor block has only one predecessor.

                    assert!(new_indices.insert(target.block_index, new_index).is_none());
                    let mut target_block = basic_blocks.remove(&target.block_index).unwrap();

                    basic_block.stmts.extend(target_block.stmts);
                    basic_block.successor = target_block.successor;
                } else {
                    break;
                }
            }
            new_basic_blocks.push(basic_block);
        }
    }
    for basic_block in &mut new_basic_blocks {
        match &mut basic_block.successor {
            vir::cfg::Successor::Undefined | vir::cfg::Successor::Return => {},
            vir::cfg::Successor::Goto(target) => {
                target.block_index = new_indices[&target.block_index];
            }
            vir::cfg::Successor::GotoSwitch(conditional_targets, default_target) => {
                default_target.block_index = new_indices[&default_target.block_index];
                for (_, target) in conditional_targets {
                    target.block_index = new_indices[&target.block_index];
                }
            }
        }
    }
    method.basic_blocks = new_basic_blocks;
    method
}