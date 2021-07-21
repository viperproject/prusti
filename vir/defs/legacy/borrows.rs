// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::ast::{Expr, Stmt};
use std::collections::{HashMap, VecDeque};
use std::fmt;

/// The method-unique borrow identifier.
#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct Borrow(usize);

impl Borrow {
    // FIXME: this constructor is currently only used for conversion
    pub fn new(index: usize) -> Self {
        Borrow(index)
    }
}

impl fmt::Debug for Borrow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "L{}", self.0)
    }
}

/// Node of the reborrowing DAG.
#[derive(Clone, PartialEq, Eq)]
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

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.borrow)
    }
}

/// Reborrowing directed acyclic graph (DAG). It should not be mutated
/// after it is constructed. For construction use `DAGBuilder`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DAG {
    /// Mapping from borrows to their node indices.
    #[serde(skip)]
    borrow_indices: HashMap<Borrow, usize>,
    #[serde(skip)]
    nodes: Vec<Node>,
    #[serde(skip)]
    borrowed_places: Vec<Expr>,
}

impl DAG {
    // FIXME: this constructor is currently only used for conversion
    pub fn new(borrow_indices: HashMap<Borrow, usize>, nodes: Vec<Node>, borrowed_places: Vec<Expr>) -> Self {
        DAG {
            borrow_indices: borrow_indices,
            nodes: nodes,
            borrowed_places: borrowed_places,
        }
    }
}

/// A struct for constructing the reborrowing DAG.
pub struct DAGBuilder {
    dag: DAG,
}

impl fmt::Debug for DAG {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ReborrowingDAG(")?;
        for node in &self.nodes {
            write!(f, "{:?},", node.borrow)?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for DAG {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ReborrowingDAG")
    }
}
