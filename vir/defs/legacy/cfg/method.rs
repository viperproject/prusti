// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::iter::FromIterator;
use uuid::Uuid;
use crate::legacy::ast::*;

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

    // FIXME: should not allow such constructor to be publicly accessible (currently for conversion)
    pub fn new_copy(uuid: Uuid, method_name: String, formal_arg_count: usize, formal_returns: Vec<LocalVar>, local_vars: Vec<LocalVar>, labels: HashSet<String>, 
        reserved_labels: HashSet<String>, basic_blocks: Vec<CfgBlock>, basic_blocks_labels: Vec<String>, fresh_var_index: i32, fresh_label_index: i32) -> Self {
        CfgMethod {
            uuid: uuid,
            method_name: method_name,
            formal_arg_count: formal_arg_count,
            formal_returns: formal_returns,
            local_vars: local_vars,
            labels: labels,
            reserved_labels: reserved_labels,
            basic_blocks: basic_blocks,
            basic_blocks_labels:basic_blocks_labels,
            fresh_var_index: fresh_var_index,
            fresh_label_index: fresh_label_index,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgBlock {
    pub stmts: Vec<Stmt>, // FIXME: Hack, should be pub(super).
    pub(in super::super) successor: Successor,
}

impl CfgBlock {
    // FIXME: should not allow such constructor to be publicly accessible (currently for conversion)
    pub fn new(
        stmts: Vec<Stmt>,
        successor: Successor,
    ) -> Self {
        CfgBlock {
            stmts: stmts,
            successor: successor,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Successor {
    Undefined,
    Return,
    Goto(CfgBlockIndex),
    GotoSwitch(Vec<(Expr, CfgBlockIndex)>, CfgBlockIndex),
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

impl CfgBlockIndex {
    // FIXME: should not allow such constructor to be publicly accessible (currently for conversion)
    pub fn new(
        method_uuid: Uuid,
        block_index: usize,
    ) -> Self {
        CfgBlockIndex {
            method_uuid: method_uuid,
            block_index: block_index,
        }
    }
}