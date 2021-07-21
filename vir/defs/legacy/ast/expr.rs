// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::super::borrows::Borrow;
use crate::legacy::ast::*;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::mem;
use std::mem::discriminant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    /// A local var
    Local(LocalVar, Position),
    /// An enum variant: base, variant index.
    Variant(Box<Expr>, Field, Position),
    /// A field access
    Field(Box<Expr>, Field, Position),
    /// The inverse of a `val_ref` field access
    AddrOf(Box<Expr>, Type, Position),
    LabelledOld(String, Box<Expr>, Position),
    Const(Const, Position),
    /// lhs, rhs, borrow, position
    MagicWand(Box<Expr>, Box<Expr>, Option<Borrow>, Position),
    /// PredicateAccessPredicate: predicate_name, arg, permission amount
    PredicateAccessPredicate(String, Box<Expr>, PermAmount, Position),
    FieldAccessPredicate(Box<Expr>, PermAmount, Position),
    UnaryOp(UnaryOpKind, Box<Expr>, Position),
    BinOp(BinOpKind, Box<Expr>, Box<Expr>, Position),
    /// Container Operation on a Viper container (e.g. Seq index)
    ContainerOp(ContainerOpKind, Box<Expr>, Box<Expr>, Position),
    /// Viper Seq
    Seq(Type, Vec<Expr>, Position),
    /// Unfolding: predicate name, predicate_args, in_expr, permission amount, enum variant
    Unfolding(String, Vec<Expr>, Box<Expr>, PermAmount, MaybeEnumVariantIndex, Position),
    /// Cond: guard, then_expr, else_expr
    Cond(Box<Expr>, Box<Expr>, Box<Expr>, Position),
    /// ForAll: variables, triggers, body
    ForAll(Vec<LocalVar>, Vec<Trigger>, Box<Expr>, Position),
    /// Exists: variables, triggers, body
    Exists(Vec<LocalVar>, Vec<Trigger>, Box<Expr>, Position),
    /// let variable == (expr) in body
    LetExpr(LocalVar, Box<Expr>, Box<Expr>, Position),
    /// FuncApp: function_name, args, formal_args, return_type, Viper position
    FuncApp(String, Vec<Expr>, Vec<LocalVar>, Type, Position),
    /// Domain function application: function_name, args, formal_args, return_type, domain_name, Viper position (unused)
    DomainFuncApp(DomainFunc, Vec<Expr>, Position),
    // TODO use version below once providing a return type is supported in silver
    // DomainFuncApp(String, Vec<Expr>, Vec<LocalVar>, Type, String, Position),
    /// Inhale Exhale: inhale expression, exhale expression, Viper position (unused)
    InhaleExhale(Box<Expr>, Box<Expr>, Position),
    /// Inform the fold-unfold algorithm that at this program point a enum type can be downcasted
    /// to one of its variants. This statement is a no-op for Viper.
    /// Arguments:
    /// * expression in which the downcast is visible
    /// * place to the enumeration that is downcasted
    /// * field that encodes the variant
    Downcast(Box<Expr>, Box<Expr>, Field),
    /// Snapshot call to convert from a Ref to a snapshot value
    SnapApp(Box<Expr>, Position),
}

/// A component that can be used to represent a place as a vector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlaceComponent {
    Field(Field, Position),
    Variant(Field, Position),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOpKind {
    Not,
    Minus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinOpKind {
    EqCmp,
    NeCmp,
    GtCmp,
    GeCmp,
    LtCmp,
    LeCmp,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Implies,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContainerOpKind {
    SeqIndex,
    SeqConcat,
    SeqLen,
    // more to follow if required
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Const {
    Bool(bool),
    Int(i64),
    BigInt(String),
    /// All function pointers share the same constant, because their function
    /// is determined by the type system.
    FnPtr,
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Local(ref v, ref _pos) => write!(f, "{}", v),
            Expr::Variant(ref base, ref variant_index, ref _pos) => {
                write!(f, "{}[{}]", base, variant_index)
            }
            Expr::Field(ref base, ref field, ref _pos) => write!(f, "{}.{}", base, field),
            Expr::AddrOf(ref base, _, ref _pos) => write!(f, "&({})", base),
            Expr::Const(ref value, ref _pos) => write!(f, "{}", value),
            Expr::BinOp(op, ref left, ref right, ref _pos) => {
                write!(f, "({}) {} ({})", left, op, right)
            }
            Expr::ContainerOp(op, box ref left, box ref right, _) => {
                match op {
                    ContainerOpKind::SeqIndex => write!(f, "{}[{}]", left, right),
                    ContainerOpKind::SeqConcat => write!(f, "{} ++ {}", left, right),
                    ContainerOpKind::SeqLen => write!(f, "|{}|", left),
                }
            }
            Expr::Seq(ty, elems, _) => {
                let elems_printed = elems.iter().map(|e| format!("{}", e)).collect::<Vec<_>>().join(", ");
                let elem_ty = if let Type::Seq(box elem_ty) = ty { elem_ty } else { unreachable!() };
                write!(f, "Seq[{}]({})", elem_ty, elems_printed)
            }
            Expr::UnaryOp(op, ref expr, ref _pos) => write!(f, "{}({})", op, expr),
            Expr::PredicateAccessPredicate(ref pred_name, ref arg, perm, ref _pos) => {
                write!(f, "acc({}({}), {})", pred_name, arg, perm)
            }
            Expr::FieldAccessPredicate(ref expr, perm, ref _pos) => {
                write!(f, "acc({}, {})", expr, perm)
            }
            Expr::LabelledOld(ref label, ref expr, ref _pos) => {
                write!(f, "old[{}]({})", label, expr)
            }
            Expr::MagicWand(ref left, ref right, ref borrow, ref _pos) => {
                write!(f, "({}) {:?} --* ({})", left, borrow, right)
            }
            Expr::Unfolding(ref pred_name, ref args, ref expr, perm, ref variant, ref _pos) => {
                write!(
                    f,
                    "(unfolding acc({}({}), {}) in {})",
                    if let Some(variant_index) = variant {
                        format!("{}<variant {}>", pred_name, variant_index)
                    } else {
                        format!("{}", pred_name)
                    },
                    args.iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<String>>()
                        .join(", "),
                    perm,
                    expr
                )
            },
            Expr::Cond(ref guard, ref left, ref right, ref _pos) => {
                write!(f, "({})?({}):({})", guard, left, right)
            }
            Expr::ForAll(ref vars, ref triggers, ref body, ref _pos) => write!(
                f,
                "forall {} {} :: {}",
                vars.iter()
                    .map(|x| format!("{:?}", x))
                    .collect::<Vec<String>>()
                    .join(", "),
                triggers
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                body.to_string()
            ),
            Expr::Exists(ref vars, ref triggers, ref body, ref _pos) => write!(
                f,
                "exists {} {} :: {}",
                vars.iter()
                    .map(|x| format!("{:?}", x))
                    .collect::<Vec<String>>()
                    .join(", "),
                triggers
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                body.to_string()
            ),
            Expr::LetExpr(ref var, ref expr, ref body, ref _pos) => write!(
                f,
                "(let {:?} == ({}) in {})",
                var,
                expr.to_string(),
                body.to_string()
            ),
            Expr::FuncApp(ref name, ref args, ref params, ref typ, ref _pos) => write!(
                f,
                "{}<{},{}>({})",
                name,
                params
                    .iter()
                    .map(|p| p.typ.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                typ.to_string(),
                args.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
            ),
            /* TODO include once the def. of DomainFuncApp changes
            Expr::DomainFuncApp(ref name, ref args, ref params, ref typ, ref domain_name, ref _pos) => write!(
                f,
                "{}::{}<{},{}>({})",
                domain_name,
                name,
                params
                    .iter()
                    .map(|p| p.typ.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                typ.to_string(),
                args.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
            ),
             */
            Expr::DomainFuncApp(ref function, ref args, ref _pos) => write!(
                f,
                "{}({})",
                function.name,
                args.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
            ),

            Expr::InhaleExhale(ref inhale_expr, ref exhale_expr, _) =>
                write!(f, "[({}), ({})]", inhale_expr, exhale_expr),

            Expr::Downcast(ref base, ref enum_place, ref field) => write!(
                f,
                "(downcast {} to {} in {})",
                enum_place.to_string(),
                field,
                base.to_string(),
            ),
            Expr::SnapApp(ref expr, _) => write!(f, "snap({})", expr.to_string()),
        }
    }
}

impl fmt::Display for UnaryOpKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UnaryOpKind::Not => write!(f, "!"),
            UnaryOpKind::Minus => write!(f, "-"),
        }
    }
}

impl fmt::Display for BinOpKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BinOpKind::EqCmp => write!(f, "=="),
            BinOpKind::NeCmp => write!(f, "!="),
            BinOpKind::GtCmp => write!(f, ">"),
            BinOpKind::GeCmp => write!(f, ">="),
            BinOpKind::LtCmp => write!(f, "<"),
            BinOpKind::LeCmp => write!(f, "<="),
            BinOpKind::Add => write!(f, "+"),
            BinOpKind::Sub => write!(f, "-"),
            BinOpKind::Mul => write!(f, "*"),
            BinOpKind::Div => write!(f, "\\"),
            BinOpKind::Mod => write!(f, "%"),
            BinOpKind::And => write!(f, "&&"),
            BinOpKind::Or => write!(f, "||"),
            BinOpKind::Implies => write!(f, "==>"),
        }
    }
}

impl fmt::Display for Const {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Const::Bool(val) => write!(f, "{}", val),
            Const::Int(val) => write!(f, "{}", val),
            Const::BigInt(ref val) => write!(f, "{}", val),
            Const::FnPtr => write!(f, "FnPtr"),
        }
    }
}

impl PartialEq for Expr {
    /// Compare ignoring the `position` field
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Expr::Local(ref self_var, _), Expr::Local(ref other_var, _)) => self_var == other_var,
            (
                Expr::Variant(box ref self_base, ref self_variant, _),
                Expr::Variant(box ref other_base, ref other_variant, _),
            ) => (self_base, self_variant) == (other_base, other_variant),
            (
                Expr::Field(box ref self_base, ref self_field, _),
                Expr::Field(box ref other_base, ref other_field, _),
            ) => (self_base, self_field) == (other_base, other_field),
            (
                Expr::AddrOf(box ref self_base, ref self_typ, _),
                Expr::AddrOf(box ref other_base, ref other_typ, _),
            ) => (self_base, self_typ) == (other_base, other_typ),
            (
                Expr::LabelledOld(ref self_label, box ref self_base, _),
                Expr::LabelledOld(ref other_label, box ref other_base, _),
            ) => (self_label, self_base) == (other_label, other_base),
            (Expr::Const(ref self_const, _), Expr::Const(ref other_const, _)) => {
                self_const == other_const
            }
            (
                Expr::MagicWand(box ref self_lhs, box ref self_rhs, self_borrow, _),
                Expr::MagicWand(box ref other_lhs, box ref other_rhs, other_borrow, _),
            ) => (self_lhs, self_rhs, self_borrow) == (other_lhs, other_rhs, other_borrow),
            (
                Expr::PredicateAccessPredicate(ref self_name, ref self_arg, self_perm, _),
                Expr::PredicateAccessPredicate(ref other_name, ref other_arg, other_perm, _),
            ) => (self_name, self_arg, self_perm) == (other_name, other_arg, other_perm),
            (
                Expr::FieldAccessPredicate(box ref self_base, self_perm, _),
                Expr::FieldAccessPredicate(box ref other_base, other_perm, _),
            ) => (self_base, self_perm) == (other_base, other_perm),
            (
                Expr::UnaryOp(self_op, box ref self_arg, _),
                Expr::UnaryOp(other_op, box ref other_arg, _),
            ) => (self_op, self_arg) == (other_op, other_arg),
            (
                Expr::BinOp(self_op, box ref self_left, box ref self_right, _),
                Expr::BinOp(other_op, box ref other_left, box ref other_right, _),
            ) => (self_op, self_left, self_right) == (other_op, other_left, other_right),
            (
                Expr::ContainerOp(self_op, box ref self_left, box ref self_right, _),
                Expr::ContainerOp(other_op, box ref other_left, box ref other_right, _),
            ) => (self_op, self_left, self_right) == (other_op, other_left, other_right),
            (
                Expr::Seq(self_ty, self_elems, _), Expr::Seq(other_ty, other_elems, _)
            ) => (self_ty, self_elems) == (other_ty, other_elems),
            (
                Expr::Cond(box ref self_cond, box ref self_then, box ref self_else, _),
                Expr::Cond(box ref other_cond, box ref other_then, box ref other_else, _),
            ) => (self_cond, self_then, self_else) == (other_cond, other_then, other_else),
            (
                Expr::ForAll(ref self_vars, ref self_triggers, box ref self_expr, _),
                Expr::ForAll(ref other_vars, ref other_triggers, box ref other_expr, _),
            ) => (self_vars, self_triggers, self_expr) == (other_vars, other_triggers, other_expr),
            (
                Expr::Exists(ref self_vars, ref self_triggers, box ref self_expr, _),
                Expr::Exists(ref other_vars, ref other_triggers, box ref other_expr, _),
            ) => (self_vars, self_triggers, self_expr) == (other_vars, other_triggers, other_expr),
            (
                Expr::LetExpr(ref self_var, box ref self_def, box ref self_expr, _),
                Expr::LetExpr(ref other_var, box ref other_def, box ref other_expr, _),
            ) => (self_var, self_def, self_expr) == (other_var, other_def, other_expr),
            (
                Expr::FuncApp(ref self_name, ref self_args, _, _, _),
                Expr::FuncApp(ref other_name, ref other_args, _, _, _),
            ) => (self_name, self_args) == (other_name, other_args),
            (
                Expr::Unfolding(ref self_name, ref self_args, box ref self_base, self_perm, ref self_variant, _),
                Expr::Unfolding(ref other_name, ref other_args, box ref other_base, other_perm, ref other_variant, _),
            ) => {
                (self_name, self_args, self_base, self_perm, self_variant)
                    == (other_name, other_args, other_base, other_perm, other_variant)
            }
            (
                Expr::DomainFuncApp(ref self_function_name, ref self_args, _),
                Expr::DomainFuncApp(ref other_function_name, ref other_args, _)
            ) => {
                (self_function_name, self_args)
                    == (other_function_name, other_args)
            }
            (
                Expr::SnapApp(ref self_expr, _),
                Expr::SnapApp(ref other_expr, _),
            ) => self_expr == other_expr,
            (a, b) => {
                debug_assert_ne!(discriminant(a), discriminant(b));
                false
            }
        }
    }
}

impl Eq for Expr {}

impl Hash for Expr {
    /// Hash ignoring the `position` field
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Expr::Local(ref var, _) => var.hash(state),
            Expr::Variant(box ref base, variant_index, _) => (base, variant_index).hash(state),
            Expr::Field(box ref base, ref field, _) => (base, field).hash(state),
            Expr::AddrOf(box ref base, ref typ, _) => (base, typ).hash(state),
            Expr::LabelledOld(ref label, box ref base, _) => (label, base).hash(state),
            Expr::Const(ref const_expr, _) => const_expr.hash(state),
            Expr::MagicWand(box ref lhs, box ref rhs, b, _) => (lhs, rhs, b).hash(state),
            Expr::PredicateAccessPredicate(ref name, ref arg, perm, _) => {
                (name, arg, perm).hash(state)
            }
            Expr::FieldAccessPredicate(box ref base, perm, _) => (base, perm).hash(state),
            Expr::UnaryOp(op, box ref arg, _) => (op, arg).hash(state),
            Expr::BinOp(op, box ref left, box ref right, _) => (op, left, right).hash(state),
            Expr::Cond(box ref cond, box ref then_expr, box ref else_expr, _) => {
                (cond, then_expr, else_expr).hash(state)
            }
            Expr::ForAll(ref vars, ref triggers, box ref expr, _) => {
                (vars, triggers, expr).hash(state)
            }
            Expr::Exists(ref vars, ref triggers, box ref expr, _) => {
                (vars, triggers, expr).hash(state)
            }
            Expr::LetExpr(ref var, box ref def, box ref expr, _) => (var, def, expr).hash(state),
            Expr::FuncApp(ref name, ref args, _, _, _) => (name, args).hash(state),
            Expr::DomainFuncApp(ref function, ref args, _) => (&function.name, args).hash(state),
            // TODO Expr::DomainFuncApp(ref name, ref args, _, _, ref domain_name ,_) => (name, args, domain_name).hash(state),
            Expr::Unfolding(ref name, ref args, box ref base, perm, ref variant, _) => {
                (name, args, base, perm, variant).hash(state)
            }
            Expr::InhaleExhale(box ref inhale_expr, box ref exhale_expr, _) => {
                (inhale_expr, exhale_expr).hash(state)
            }
            Expr::Downcast(box ref base, box ref enum_place, ref field) => {
                (base, enum_place, field).hash(state)
            }
            Expr::SnapApp(ref expr, _) => expr.hash(state),
            Expr::ContainerOp(op_kind, box ref left, box ref right, _) => {
                (op_kind, left, right).hash(state)
            }
            Expr::Seq(ty, elems, _) => {
                (ty, elems).hash(state)
            }
        }
    }
}
