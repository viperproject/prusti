// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::config;
use std::collections::HashMap;
use viper::{self, AstFactory};
use crate::vir::{
    ast::Position,
    cfg::method::*,
    to_viper::{ToViper, ToViperDecl},
};

impl<'v> ToViper<'v, viper::Method<'v>> for CfgMethod {
    fn to_viper(&self, ast: &AstFactory<'v>) -> viper::Method<'v> {
        (&self).to_viper(ast)
    }
}

impl<'a, 'v> ToViper<'v, viper::Method<'v>> for &'a CfgMethod {
    fn to_viper(&self, ast: &AstFactory<'v>) -> viper::Method<'v> {
        let mut blocks_ast: Vec<viper::Stmt> = vec![];
        let mut declarations: Vec<viper::Declaration> = vec![];

        for local_var in self.local_vars.iter() {
            declarations.push(local_var.to_viper_decl(ast).into());
        }
        for label in self.labels.iter() {
            let decl = ast.label(label, &[]);
            declarations.push(decl.into());
        }

        if config::enable_verify_only_basic_block_path() {
            let path = config::verify_only_basic_block_path();
            self.convert_basic_block_path(path, ast, &mut blocks_ast, &mut declarations);
        } else {
            // Sort blocks by label, except for the first block
            let mut blocks: Vec<_> = self.basic_blocks.iter().enumerate().skip(1).collect();
            blocks.sort_by_key(|(index, _)| index_to_label(&self.basic_blocks_labels, *index));
            blocks.insert(0, (0, &self.basic_blocks[0]));

            for (index, block) in blocks.into_iter() {
                blocks_ast.push(block_to_viper(ast, &self.basic_blocks_labels, block, index));
                declarations.push(
                    ast.label(&index_to_label(&self.basic_blocks_labels, index), &[])
                        .into(),
                );
            }
        }
        blocks_ast.push(ast.label(RETURN_LABEL, &[]));
        declarations.push(ast.label(RETURN_LABEL, &[]).into());

        let method_body = Some(ast.seqn(&blocks_ast, &declarations));

        let mut formal_returns_decl: Vec<viper::LocalVarDecl> = vec![];
        for local_var in &self.formal_returns {
            formal_returns_decl.push(local_var.to_viper_decl(ast));
        }

        let method = ast.method(
            &self.method_name,
            &[],
            &formal_returns_decl,
            &[],
            &[],
            method_body,
        );

        method
    }
}

impl CfgMethod {
    fn convert_basic_block_path<'v>(
        &self,
        mut path: Vec<String>,
        ast: &'v AstFactory,
        blocks_ast: &mut Vec<viper::Stmt<'v>>,
        declarations: &mut Vec<viper::Declaration<'v>>,
    ) {
        path.reverse();
        let mut remaining_blocks: HashMap<_, _> = self
            .basic_blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                (
                    index_to_label(&self.basic_blocks_labels, index),
                    (index, block),
                )
            })
            .collect();
        let mut current_label = index_to_label(&self.basic_blocks_labels, 0);
        while let Some((index, block)) = remaining_blocks.remove(&current_label) {
            blocks_ast.push(block_to_viper(ast, &self.basic_blocks_labels, block, index));
            declarations.push(
                ast.label(&index_to_label(&self.basic_blocks_labels, index), &[])
                    .into(),
            );

            let mut successors: Vec<_> = block
                .successor
                .get_following()
                .into_iter()
                .map(|ci| index_to_label(&self.basic_blocks_labels, ci.block_index))
                .collect();
            assert!(!successors.is_empty());

            if successors.len() == 1 {
                current_label = successors.pop().unwrap();
            } else if let Some(next_label) = path.pop() {
                current_label = next_label;
                assert!(
                    successors.contains(&current_label),
                    "successors: {:?} next_label: {:?}",
                    successors,
                    current_label
                );
            } else {
                break;
            }
        }

        for label in config::delete_basic_blocks() {
            let (index, block) = remaining_blocks.remove(&label).unwrap();
            let fake_position = Position::default();
            let stmts: Vec<viper::Stmt> = vec![
                ast.label(&label, &[]),
                ast.inhale(
                    ast.false_lit_with_pos(fake_position.to_viper(ast)),
                    fake_position.to_viper(ast),
                ),
                successor_to_viper(ast, index, &self.basic_blocks_labels, &block.successor),
            ];
            blocks_ast.push(ast.seqn(&stmts, &[]));
            declarations.push(ast.label(&label, &[]).into());
        }

        for (label, (index, block)) in remaining_blocks {
            blocks_ast.push(block_to_viper(ast, &self.basic_blocks_labels, block, index));
            declarations.push(ast.label(&label, &[]).into());
        }
    }
}

impl<'v> ToViper<'v, Vec<viper::Method<'v>>> for Vec<CfgMethod> {
    fn to_viper(&self, ast: &AstFactory<'v>) -> Vec<viper::Method<'v>> {
        self.iter().map(|x| x.to_viper(ast)).collect()
    }
}

fn index_to_label(basic_block_labels: &Vec<String>, index: usize) -> String {
    basic_block_labels[index].clone()
}

fn successor_to_viper<'a>(
    ast: &'a AstFactory,
    index: usize,
    basic_block_labels: &Vec<String>,
    successor: &Successor,
) -> viper::Stmt<'a> {
    match *successor {
        Successor::Undefined => panic!(
            "CFG block '{}' has no successor.",
            basic_block_labels[index].clone()
        ),
        Successor::Return => ast.goto(RETURN_LABEL),
        Successor::Goto(target) => ast.goto(&basic_block_labels[target.block_index]),
        Successor::GotoSwitch(ref successors, ref default_target) => {
            let mut stmts: Vec<viper::Stmt<'a>> = vec![];
            for (test, target) in successors {
                let goto = ast.goto(&basic_block_labels[target.block_index]);
                let skip = ast.seqn(&[], &[]);
                let conditional_goto = ast.if_stmt(test.to_viper(ast), goto, skip);
                stmts.push(conditional_goto);
            }
            let default_goto = ast.goto(&basic_block_labels[default_target.block_index]);
            stmts.push(default_goto);
            ast.seqn(&stmts, &[])
        }
    }
}

fn block_to_viper<'a>(
    ast: &'a AstFactory,
    basic_block_labels: &Vec<String>,
    block: &CfgBlock,
    index: usize,
) -> viper::Stmt<'a> {
    let label = &basic_block_labels[index];
    let mut stmts: Vec<viper::Stmt> = vec![];
    stmts.push(ast.label(label, &[]));
    stmts.extend(block.stmts.to_viper(ast));
    stmts.push(successor_to_viper(
        ast,
        index,
        basic_block_labels,
        &block.successor,
    ));
    ast.seqn(&stmts, &[])
}
