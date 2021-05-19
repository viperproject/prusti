// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::*;
use std::collections::{HashMap};

/// The method-unique borrow identifier.
#[derive(Debug, Clone, Copy)]
pub struct Borrow(usize);

/// Node of the reborrowing DAG.
#[derive(Debug, Clone)]
pub struct Node {
    /// The basic block at which the borrow occured was executed only
    /// iff the `guard` is true.
    pub guard: Expr,
    pub borrow: Borrow,
    pub reborrowing_nodes: Vec<Borrow>,
    pub reborrowed_nodes: Vec<Borrow>,
    pub stmts: Vec<Stmt>,
    /// Places that were borrowed and should be kept in fold/unfold.
    pub borrowed_places: Vec<Expr>,
    /// Borrows that are borrowing the same place.
    pub conflicting_borrows: Vec<Borrow>,
    pub alive_conflicting_borrows: Vec<Borrow>,
    /// The place (potentially old) through which the permissions can
    /// still be accessed even if the loan was killed.
    pub place: Option<Expr>,
}

/// Reborrowing directed acyclic graph (DAG). It should not be mutated
/// after it is constructed. For construction use `DAGBuilder`.
#[derive(Debug, Clone)]
pub struct DAG {
    /// Mapping from borrows to their node indices.
    borrow_indices: HashMap<Borrow, usize>,
    nodes: Vec<Node>,
    borrowed_places: Vec<Expr>,
}

/// A struct for constructing the reborrowing DAG.
pub struct DAGBuilder {
    dag: DAG,
}
