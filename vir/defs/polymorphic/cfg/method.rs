// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::iter::FromIterator;
use uuid::Uuid;
use crate::polymorphic::ast::*;

use super::super::super::legacy;

pub(super) const RETURN_LABEL: &str = "end_of_method";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgMethod {
    // TODO: extract logic using (most) skipped fields to CfgMethodBuilder
    #[serde(skip)]
    pub(super) uuid: Uuid,
    pub(super) method_name: String,
    pub(in super::super) formal_arg_count: usize,
    pub(in super::super) formal_returns: Vec<LocalVar>,
    // FIXME: This should be pub(in super::super). However, the optimization
    // that depends on snapshots needs to modify this field.
    pub local_vars: Vec<LocalVar>,
    pub(super) labels: HashSet<String>,
    #[serde(skip)]
    pub(super) reserved_labels: HashSet<String>,
    pub basic_blocks: Vec<CfgBlock>, // FIXME: Hack, should be pub(super).
    pub(super) basic_blocks_labels: Vec<String>,
    #[serde(skip)]
    fresh_var_index: i32,
    #[serde(skip)]
    fresh_label_index: i32,
}

impl CfgMethod {
    pub fn new(
        method_name: String,
        formal_arg_count: usize,
        formal_returns: Vec<LocalVar>,
        local_vars: Vec<LocalVar>,
        reserved_labels: Vec<String>,
    ) -> Self {
        CfgMethod {
            uuid: Uuid::new_v4(),
            method_name,
            formal_arg_count,
            formal_returns,
            local_vars,
            labels: HashSet::new(),
            reserved_labels: HashSet::from_iter(reserved_labels),
            basic_blocks: vec![],
            basic_blocks_labels: vec![],
            fresh_var_index: 0,
            fresh_label_index: 0,
        }
    }
}

impl From<CfgMethod> for legacy::CfgMethod {
    fn from(cfg_method: CfgMethod) -> legacy::CfgMethod {
        legacy::CfgMethod::new_copy(
            cfg_method.uuid,
            cfg_method.method_name.clone(),
            cfg_method.formal_arg_count,
            cfg_method.formal_returns.iter().map(|formal_return| legacy::LocalVar::from(formal_return.clone())).collect(),
            cfg_method.local_vars.iter().map(|local_var| legacy::LocalVar::from(local_var.clone())).collect(),
            cfg_method.labels.iter().map(|label| label.clone()).collect(),
            cfg_method.reserved_labels.iter().map(|reserved_label| reserved_label.clone()).collect(),
            cfg_method.basic_blocks.iter().map(|basic_block| legacy::CfgBlock::from(basic_block.clone())).collect(),
            cfg_method.basic_blocks_labels.iter().map(|basic_blocks_label| basic_blocks_label.clone()).collect(),
            cfg_method.fresh_var_index,
            cfg_method.fresh_label_index,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgBlock {
    pub stmts: Vec<Stmt>, // FIXME: Hack, should be pub(super).
    pub(in super::super) successor: Successor,
}

impl From<CfgBlock> for legacy::CfgBlock {
    fn from(cfg_block: CfgBlock) -> legacy::CfgBlock {
        legacy::CfgBlock::new(
            cfg_block.stmts.iter().map(|stmt| legacy::Stmt::from(stmt.clone())).collect(),
            legacy::Successor::from(cfg_block.successor.clone()),
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Successor {
    Undefined,
    Return,
    Goto(CfgBlockIndex),
    GotoSwitch(Vec<(Expr, CfgBlockIndex)>, CfgBlockIndex),
}

impl From<Successor> for legacy::Successor {
    fn from(successor: Successor) -> legacy::Successor {
        match successor {
            Successor::Undefined => legacy::Successor::Undefined,
            Successor::Return => legacy::Successor::Return,
            Successor::Goto(cfg_block_index) => legacy::Successor::Goto(legacy::CfgBlockIndex::from(cfg_block_index.clone())),
            Successor::GotoSwitch(expr_indices, cfg_block_index) => legacy::Successor::GotoSwitch(
                expr_indices.iter().map(|(expr, index)| (legacy::Expr::from(expr.clone()), legacy::CfgBlockIndex::from(index.clone()))).collect(),
                legacy::CfgBlockIndex::from(cfg_block_index.clone()),
            ),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct CfgBlockIndex {
    #[serde(skip)]
    pub(super) method_uuid: Uuid,
    pub(in super::super) block_index: usize,
}

impl fmt::Debug for CfgBlockIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "cfg:{}", self.block_index)
    }
}

impl From<CfgBlockIndex> for legacy::CfgBlockIndex {
    fn from(cfg_block_index: CfgBlockIndex) -> legacy::CfgBlockIndex {
        legacy::CfgBlockIndex::new(
            cfg_block_index.method_uuid,
            cfg_block_index.block_index,
        )
    }
}
