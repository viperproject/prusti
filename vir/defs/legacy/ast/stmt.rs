// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::super::borrows::{Borrow, DAG as ReborrowingDAG};
use super::super::cfg::CfgBlockIndex;
use crate::legacy::ast::*;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stmt {
    Comment(String),
    Label(String),
    Inhale(Expr),
    Exhale(Expr, Position),
    Assert(Expr, Position),
    /// MethodCall: method_name, args, targets
    MethodCall(String, Vec<Expr>, Vec<LocalVar>),
    /// Target, source, kind
    Assign(Expr, Expr, AssignKind),
    /// Fold statement: predicate name, predicate args, perm_amount, variant of enum, position.
    Fold(String, Vec<Expr>, PermAmount, MaybeEnumVariantIndex, Position),
    /// Unfold statement: predicate name, predicate args, perm_amount, variant of enum.
    Unfold(String, Vec<Expr>, PermAmount, MaybeEnumVariantIndex),
    /// Obtain: conjunction of Expr::PredicateAccessPredicate or Expr::FieldAccessPredicate
    /// They will be used by the fold/unfold algorithm
    Obtain(Expr, Position),
    /// Mark a CFG point in which all current permissions are framed out
    /// They will be used by the fold/unfold algorithm
    BeginFrame,
    /// Mark a CFG point in which all the permissions of a corresponding `BeginFrame` are framed in
    /// They will be used by the fold/unfold algorithm
    EndFrame,
    /// Move permissions from a place to another.
    /// This is used to restore permissions in the fold/unfold state when a loan expires.
    ///
    /// The last argument indicates if the transfer is unchecked. Unchecked transfer is used for
    /// encoding shared borrows which can be dangling and, therefore, we cannot use the safety
    /// checks.
    TransferPerm(Expr, Expr, bool),
    /// Package a Magic Wand
    /// Arguments: the magic wand, the package statement's body, the
    /// label just before the statement, and ghost variables used inside
    /// the package statement.
    PackageMagicWand(Expr, Vec<Stmt>, String, Vec<LocalVar>, Position),
    /// Apply a Magic Wand.
    /// Arguments: the magic wand.
    ApplyMagicWand(Expr, Position),
    /// Expire borrows given in the reborrowing DAG.
    ExpireBorrows(ReborrowingDAG),
    /// An `if` statement: the guard and the 'then' branch.
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    /// Inform the fold-unfold algorithm that at this program point a enum type can be downcasted
    /// to one of its variants. This statement is a no-op for Viper.
    /// Arguments:
    /// * place to the enumeration instance
    /// * field that encodes the variant
    Downcast(Expr, Field),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssignKind {
    /// Encodes a Rust copy.
    /// This assignment can be used iff the Viper type of the `lhs` and `rhs` is *not* Ref.
    Copy,
    /// Encodes a Rust move. The permissions in the rhs move to the `lhs`.
    /// This assignment can be used iff the Viper type of the `lhs` and `rhs` is Ref.
    Move,
    /// Encodes the initialization of a mutable borrow.
    /// The permissions in the `rhs` move to the `lhs`, but they can be restored when the borrow dies.
    MutableBorrow(Borrow),
    /// Encodes the initialization of a shared borrow.
    /// The permissions in the `rhs` are duplicated to the `lhs`.
    SharedBorrow(Borrow),
    /// Used to mark that the assignment is to a ghost variable and should be ignored by
    /// the fold-unfold algorithm.
    Ghost,
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Stmt::Comment(ref comment) => write!(f, "// {}", comment),
            Stmt::Label(ref label) => write!(f, "label {}", label),
            Stmt::Inhale(ref expr) => {
                write!(f, "inhale {}", expr)
            },
            Stmt::Exhale(ref expr, _) => write!(f, "exhale {}", expr),
            Stmt::Assert(ref expr, _) => {
                write!(f, "assert {}", expr)
            },
            Stmt::MethodCall(ref name, ref args, ref vars) => write!(
                f,
                "{} := {}({})",
                vars.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                name,
                args.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
            ),
            Stmt::Assign(ref lhs, ref rhs, kind) => match kind {
                AssignKind::Move => write!(f, "{} := move {}", lhs, rhs),
                AssignKind::Copy => write!(f, "{} := copy {}", lhs, rhs),
                AssignKind::MutableBorrow(borrow) => {
                    write!(f, "{} := mut borrow {} // {:?}", lhs, rhs, borrow)
                }
                AssignKind::SharedBorrow(borrow) => {
                    write!(f, "{} := borrow {} // {:?}", lhs, rhs, borrow)
                }
                AssignKind::Ghost => write!(f, "{} := ghost {}", lhs, rhs),
            },

            Stmt::Fold(ref pred_name, ref args, perm, ref variant, _) => write!(
                f,
                "fold acc({}({}), {})",
                if let Some(variant_index) = variant {
                    format!("{}<variant {}>", pred_name, variant_index)
                } else {
                    format!("{}", pred_name)
                },
                args.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                perm,
            ),

            Stmt::Unfold(ref pred_name, ref args, perm, ref variant) => write!(
                f,
                "unfold acc({}({}), {})",
                if let Some(variant_index) = variant {
                    format!("{}<variant {}>", pred_name, variant_index)
                } else {
                    format!("{}", pred_name)
                },
                args.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                perm,
            ),

            Stmt::Obtain(ref expr, _) => write!(f, "obtain {}", expr),

            Stmt::BeginFrame => write!(f, "begin frame"),

            Stmt::EndFrame => write!(f, "end frame"),

            Stmt::TransferPerm(ref lhs, ref rhs, unchecked) => write!(
                f,
                "transfer perm {} --> {} // unchecked: {}",
                lhs, rhs, unchecked
            ),

            Stmt::PackageMagicWand(
                ref magic_wand,
                ref package_stmts,
                ref label,
                _vars,
                _position,
            ) => {
                if let Expr::MagicWand(ref lhs, ref rhs, None, _) = magic_wand {
                    writeln!(f, "package[{}] {}", label, lhs)?;
                    writeln!(f, "    --* {}", rhs)?;
                } else {
                    writeln!(f, "package[{}] {}", label, magic_wand)?;
                }
                write!(f, "{{")?;
                if !package_stmts.is_empty() {
                    write!(f, "\n")?;
                }
                for stmt in package_stmts.iter() {
                    writeln!(f, "    {}", stmt.to_string().replace("\n", "\n    "))?;
                }
                write!(f, "}}")
            }

            Stmt::ApplyMagicWand(Expr::MagicWand(ref lhs, ref rhs, Some(borrow), _), _) => {
                writeln!(f, "apply[{:?}] {} --* {}", borrow, lhs, rhs)
            }

            Stmt::ApplyMagicWand(ref magic_wand, _) => {
                if let Expr::MagicWand(ref lhs, ref rhs, Some(borrow), _) = magic_wand {
                    writeln!(f, "apply[{:?}] {} --* {}", borrow, lhs, rhs)
                } else {
                    writeln!(f, "apply {}", magic_wand)
                }
            }

            Stmt::ExpireBorrows(dag) => writeln!(f, "expire_borrows {:?}", dag),

            Stmt::If(ref guard, ref then_stmts, ref else_stmts) => {
                fn write_stmt(f: &mut fmt::Formatter, stmt: &Stmt) -> fmt::Result {
                    writeln!(f, "    {}", stmt.to_string().replace("\n", "\n    "))
                }
                fn write_block(f: &mut fmt::Formatter, stmts: &[Stmt]) -> fmt::Result {
                    write!(f, "{{")?;
                    if !stmts.is_empty() {
                        write!(f, "\n")?;
                    }
                    for stmt in stmts.iter() {
                        write_stmt(f, stmt)?;
                    }
                    write!(f, "}}")
                }
                write!(f, "if {} ", guard)?;
                write_block(f, then_stmts)?;
                write!(f, " else ")?;
                write_block(f, else_stmts)
            }

            Stmt::Downcast(e, v) => writeln!(f, "downcast {} to {}", e, v),
        }
    }
}
