// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::ast::{Expr, Stmt};
use std::collections::{HashMap, VecDeque};
use std::fmt;

use super::super::legacy;

/// The method-unique borrow identifier.
#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct Borrow(usize);

impl fmt::Debug for Borrow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "L{}", self.0)
    }
}

impl From<Borrow> for legacy::Borrow {
    fn from(borrow: Borrow) -> legacy::Borrow {
        legacy::Borrow::new(borrow.0)
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

impl From<Node> for legacy::Node {
    fn from(node: Node) -> legacy::Node {
        legacy::Node {
            guard: legacy::Expr::from(node.guard.clone()),
            borrow: legacy::Borrow::from(node.borrow.clone()),
            reborrowing_nodes: node.reborrowing_nodes.iter().map(|reborrowing_node| legacy::Borrow::from(reborrowing_node.clone())).collect(),
            reborrowed_nodes: node.reborrowed_nodes.iter().map(|reborrowed_node| legacy::Borrow::from(reborrowed_node.clone())).collect(),
            stmts: node.stmts.iter().map(|stmt| legacy::Stmt::from(stmt.clone())).collect(),
            borrowed_places: node.borrowed_places.iter().map(|borrowed_place| legacy::Expr::from(borrowed_place.clone())).collect(),
            conflicting_borrows: node.conflicting_borrows.iter().map(|conflicting_borrow| legacy::Borrow::from(conflicting_borrow.clone())).collect(),
            alive_conflicting_borrows: node.alive_conflicting_borrows.iter().map(|alive_conflicting_borrow| legacy::Borrow::from(alive_conflicting_borrow.clone())).collect(),
            place: match node.place {
                Some(expr) => Some(legacy::Expr::from(expr.clone())),
                _ => None,
            },
        }
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

impl From<DAG> for legacy::DAG {
    fn from(dag: DAG) -> legacy::DAG {
        legacy::DAG::new (
            dag.borrow_indices.iter().map(|(borrow, index)| (legacy::Borrow::from(borrow.clone()), *index)).collect(),
            dag.nodes.iter().map(|node| legacy::Node::from(node.clone())).collect(),
            dag.borrowed_places.iter().map(|borrowed_place| legacy::Expr::from(borrowed_place.clone())).collect(),
        )
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
