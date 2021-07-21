// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::super::borrows::Borrow;
use crate::polymorphic::ast::*;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::mem;
use std::mem::discriminant;

use super::super::super::legacy;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Expr {
    /// A local var
    Local(Local),
    /// An enum variant: base, variant index.
    Variant(Variant),
    /// A field access
    Field(FieldExpr),
    /// The inverse of a `val_ref` field access
    AddrOf(AddrOf),
    LabelledOld(LabelledOld),
    Const(ConstExpr),
    /// lhs, rhs, borrow, position
    MagicWand(MagicWand),
    /// PredicateAccessPredicate: predicate_name, arg, permission amount
    PredicateAccessPredicate(PredicateAccessPredicate),
    FieldAccessPredicate(FieldAccessPredicate),
    UnaryOp(UnaryOp),
    BinOp(BinOp),
    /// Container Operation on a Viper container (e.g. Seq index)
    ContainerOp(ContainerOp),
    /// Viper Seq
    Seq(Seq),
    /// Unfolding: predicate name, predicate_args, in_expr, permission amount, enum variant
    Unfolding(Unfolding),
    /// Cond: guard, then_expr, else_expr
    Cond(Cond),
    /// ForAll: variables, triggers, body
    ForAll(ForAll),
    /// Exists: variables, triggers, body
    Exists(Exists),
    /// let variable == (expr) in body
    LetExpr(LetExpr),
    /// FuncApp: function_name, args, formal_args, return_type, Viper position
    FuncApp(FuncApp),
    /// Domain function application: function_name, args, formal_args, return_type, domain_name, Viper position (unused)
    DomainFuncApp(DomainFuncApp),
    // TODO use version below once providing a return type is supported in silver
    // DomainFuncApp(String, Vec<Expr>, Vec<LocalVar>, Type, String, Position),
    /// Inhale Exhale: inhale expression, exhale expression, Viper position (unused)
    InhaleExhale(InhaleExhale),
    /// Inform the fold-unfold algorithm that at this program point a enum type can be downcasted
    /// to one of its variants. This statement is a no-op for Viper.
    /// Arguments:
    /// * expression in which the downcast is visible
    /// * place to the enumeration that is downcasted
    /// * field that encodes the variant
    Downcast(Downcast),
    /// Snapshot call to convert from a Ref to a snapshot value
    SnapApp(SnapApp),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<Expr> for legacy::Expr {
    fn from(expr: Expr) -> legacy::Expr {
        match expr {
            Expr::Local(local) => legacy::Expr::Local(
                legacy::LocalVar::from(local.variable.clone()), 
                legacy::Position::from(local.position.clone())
            ),
            Expr::Variant(variant) => legacy::Expr::Variant(
                Box::new(legacy::Expr::from(*(variant.base).clone())), 
                legacy::Field::from(variant.variant_index.clone()), 
                legacy::Position::from(variant.position.clone())
            ),
            Expr::Field(field) => legacy::Expr::Field(
                Box::new(legacy::Expr::from(*(field.base).clone())), 
                legacy::Field::from(field.field.clone()), 
                legacy::Position::from(field.position.clone())
            ),
            Expr::AddrOf(addr_of) => legacy::Expr::AddrOf(
                Box::new(legacy::Expr::from(*(addr_of.base).clone())), 
                legacy::Type::from(addr_of.addr_type.clone()),
                legacy::Position::from(addr_of.position.clone()),
            ),
            Expr::LabelledOld(labelled_old) => legacy::Expr::LabelledOld(
                labelled_old.label.clone(),
                Box::new(legacy::Expr::from(*(labelled_old.base).clone())), 
                legacy::Position::from(labelled_old.position.clone()),
            ),
            Expr::Const(const_expr) => legacy::Expr::Const(
                legacy::Const::from(const_expr.value.clone()),
                legacy::Position::from(const_expr.position.clone()),
            ),
            Expr::MagicWand(magic_wand) => legacy::Expr::MagicWand(
                Box::new(legacy::Expr::from(*(magic_wand.left).clone())), 
                Box::new(legacy::Expr::from(*(magic_wand.right).clone())), 
                match magic_wand.borrow {
                    Some(borrow) => Some(legacy::Borrow::from(borrow.clone())),
                    _ => None,
                },
                legacy::Position::from(magic_wand.position.clone()),
            ),
            Expr::PredicateAccessPredicate(predicate_access_predicate) => legacy::Expr::PredicateAccessPredicate(
                predicate_access_predicate.predicate_name.clone(),
                Box::new(legacy::Expr::from(*(predicate_access_predicate.argument).clone())),
                legacy::PermAmount::from(predicate_access_predicate.permission),
                legacy::Position::from(predicate_access_predicate.position.clone()),
            ),
            Expr::FieldAccessPredicate(field_access_predicate) => legacy::Expr::FieldAccessPredicate(
                Box::new(legacy::Expr::from(*(field_access_predicate.base).clone())),
                legacy::PermAmount::from(field_access_predicate.permission),
                legacy::Position::from(field_access_predicate.position.clone()),
            ),
            Expr::UnaryOp(unary_op) => legacy::Expr::UnaryOp(
                legacy::UnaryOpKind::from(unary_op.op_kind),
                Box::new(legacy::Expr::from(*(unary_op.argument).clone())),
                legacy::Position::from(unary_op.position.clone()),
            ),
            Expr::BinOp(bin_op) => legacy::Expr::BinOp(
                legacy::BinOpKind::from(bin_op.op_kind),
                Box::new(legacy::Expr::from(*(bin_op.left).clone())),
                Box::new(legacy::Expr::from(*(bin_op.right).clone())),
                legacy::Position::from(bin_op.position.clone()),
            ),
            Expr::ContainerOp(container_op) => legacy::Expr::ContainerOp(
                legacy::ContainerOpKind::from(container_op.op_kind),
                Box::new(legacy::Expr::from(*(container_op.left).clone())),
                Box::new(legacy::Expr::from(*(container_op.left).clone())),
                legacy::Position::from(container_op.position.clone()),
            ),
            Expr::Seq(seq) => legacy::Expr::Seq(
                legacy::Type::from(seq.typ.clone()),
                seq.elements.iter().map(|element| legacy::Expr::from(element.clone())).collect(),
                legacy::Position::from(seq.position.clone()),
            ),
            Expr::Unfolding(unfolding) => legacy::Expr::Unfolding(
                unfolding.predicate_name.clone(),
                unfolding.arguments.iter().map(|argument| legacy::Expr::from(argument.clone())).collect(),
                Box::new(legacy::Expr::from(*(unfolding.base).clone())),
                legacy::PermAmount::from(unfolding.permission),
                match unfolding.variant {
                    Some(enum_variant_index) => Some(legacy::EnumVariantIndex::from(enum_variant_index.clone())),
                    _ => None,
                },
                legacy::Position::from(unfolding.position.clone()),
            ),
            Expr::Cond(cond) => legacy::Expr::Cond(
                Box::new(legacy::Expr::from(*(cond.guard).clone())),
                Box::new(legacy::Expr::from(*(cond.then_expr).clone())),
                Box::new(legacy::Expr::from(*(cond.else_expr).clone())),
                legacy::Position::from(cond.position.clone()),
            ),
            Expr::ForAll(for_all) => legacy::Expr::ForAll(
                for_all.variables.iter().map(|variable| legacy::LocalVar::from(variable.clone())).collect(),
                for_all.triggers.iter().map(|trigger| legacy::Trigger::from(trigger.clone())).collect(),
                Box::new(legacy::Expr::from(*(for_all.body).clone())),
                legacy::Position::from(for_all.position.clone()),
            ),
            Expr::Exists(exists) => legacy::Expr::Exists(
                exists.variables.iter().map(|variable| legacy::LocalVar::from(variable.clone())).collect(),
                exists.triggers.iter().map(|trigger| legacy::Trigger::from(trigger.clone())).collect(),
                Box::new(legacy::Expr::from(*(exists.body).clone())),
                legacy::Position::from(exists.position.clone()),
            ),
            Expr::LetExpr(let_expr) => legacy::Expr::LetExpr(
                legacy::LocalVar::from(let_expr.variable.clone()), 
                Box::new(legacy::Expr::from(*(let_expr.def).clone())),
                Box::new(legacy::Expr::from(*(let_expr.body).clone())),
                legacy::Position::from(let_expr.position.clone()),
            ),
            Expr::FuncApp(func_app) => legacy::Expr::FuncApp(
                func_app.function_name.clone(),
                func_app.arguments.iter().map(|argument| legacy::Expr::from(argument.clone())).collect(),
                func_app.formal_arguments.iter().map(|formal_argument| legacy::LocalVar::from(formal_argument.clone())).collect(),
                legacy::Type::from(func_app.return_type.clone()),
                legacy::Position::from(func_app.position.clone()),
            ),
            Expr::DomainFuncApp(domain_func_app) => legacy::Expr::DomainFuncApp(
                legacy::DomainFunc::from(domain_func_app.domain_function.clone()),
                domain_func_app.arguments.iter().map(|argument| legacy::Expr::from(argument.clone())).collect(),
                legacy::Position::from(domain_func_app.position.clone()),
            ),
            Expr::InhaleExhale(inhale_exhale) => legacy::Expr::InhaleExhale(
                Box::new(legacy::Expr::from(*(inhale_exhale.inhale_expr).clone())),
                Box::new(legacy::Expr::from(*(inhale_exhale.exhale_expr).clone())),
                legacy::Position::from(inhale_exhale.position.clone()),
            ),
            Expr::Downcast(down_cast) => legacy::Expr::Downcast(
                Box::new(legacy::Expr::from(*(down_cast.base).clone())),
                Box::new(legacy::Expr::from(*(down_cast.enum_place).clone())),
                legacy::Field::from(down_cast.field.clone()),
            ),
            Expr::SnapApp(snap_app) => legacy::Expr::SnapApp(
                Box::new(legacy::Expr::from(*(snap_app.base).clone())),
                legacy::Position::from(snap_app.position.clone()),
            )
        }
    }
}

/// A component that can be used to represent a place as a vector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlaceComponent {
    Field(Field, Position),
    Variant(Field, Position),
}

impl From<PlaceComponent> for legacy::PlaceComponent {
    fn from(place_component: PlaceComponent) -> legacy::PlaceComponent {
        match place_component {
            PlaceComponent::Field(field, position) => legacy::PlaceComponent::Field(legacy::Field::from(field), legacy::Position::from(position)),
            PlaceComponent::Variant(field, position) => legacy::PlaceComponent::Variant(legacy::Field::from(field), legacy::Position::from(position)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOpKind {
    Not,
    Minus,
}

impl From<UnaryOpKind> for legacy::UnaryOpKind {
    fn from(unary_op_kind: UnaryOpKind) -> legacy::UnaryOpKind {
        match unary_op_kind {
            UnaryOpKind::Not => legacy::UnaryOpKind::Not,
            UnaryOpKind::Minus => legacy::UnaryOpKind::Minus,
        }
    }
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

impl From<BinOpKind> for legacy::BinOpKind {
    fn from(bin_op_kind: BinOpKind) -> legacy::BinOpKind {
        match bin_op_kind {
            BinOpKind::EqCmp => legacy::BinOpKind::EqCmp,
            BinOpKind::NeCmp => legacy::BinOpKind::NeCmp,
            BinOpKind::GtCmp => legacy::BinOpKind::GtCmp,
            BinOpKind::GeCmp => legacy::BinOpKind::GeCmp,
            BinOpKind::LtCmp => legacy::BinOpKind::LtCmp,
            BinOpKind::LeCmp => legacy::BinOpKind::LeCmp,
            BinOpKind::Add => legacy::BinOpKind::Add,
            BinOpKind::Sub => legacy::BinOpKind::Sub,
            BinOpKind::Mul => legacy::BinOpKind::Mul,
            BinOpKind::Div => legacy::BinOpKind::Div,
            BinOpKind::Mod => legacy::BinOpKind::Mod,
            BinOpKind::And => legacy::BinOpKind::And,
            BinOpKind::Or => legacy::BinOpKind::Or,
            BinOpKind::Implies => legacy::BinOpKind::Implies,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContainerOpKind {
    SeqIndex,
    SeqConcat,
    SeqLen,
}

impl From<ContainerOpKind> for legacy::ContainerOpKind {
    fn from(container_op_kind: ContainerOpKind) -> legacy::ContainerOpKind {
        match container_op_kind {
            ContainerOpKind::SeqIndex => legacy::ContainerOpKind::SeqIndex,
            ContainerOpKind::SeqConcat => legacy::ContainerOpKind::SeqConcat,
            ContainerOpKind::SeqLen => legacy::ContainerOpKind::SeqLen,
        }
    }
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

impl From<Const> for legacy::Const {
    fn from(old_const: Const) -> legacy::Const {
        match old_const {
            Const::Bool(bool_value) => legacy::Const::Bool(bool_value),
            Const::Int(i64_value) => legacy::Const::Int(i64_value),
            Const::BigInt(label) => legacy::Const::BigInt(label.clone()),
            Const::FnPtr => legacy::Const::FnPtr,
        }
    }
}

/// Individual structs for different cases of Expr
#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct Local {
    pub variable: LocalVar,
    pub position: Position,
}

impl fmt::Display for Local {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.variable)
    }
}

impl PartialEq for Local {
    fn eq(&self, other: &Self) -> bool {
        &self.variable == &other.variable
    }
}

impl Hash for Local {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.variable).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct Variant {
    pub base: Box<Expr>,
    pub variant_index: Field,
    pub position: Position,
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}[{}]", &self.base, &self.variant_index)
    }
}

impl PartialEq for Variant {
    fn eq(&self, other: &Self) -> bool {
        (&*self.base, &self.variant_index) == (&*other.base, &other.variant_index)
    }
}

impl Hash for Variant {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&*self.base, &self.variant_index).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct FieldExpr {
    pub base: Box<Expr>,
    pub field: Field,
    pub position: Position,
}

impl fmt::Display for FieldExpr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}", &self.base, &self.field)
    }
}

impl PartialEq for FieldExpr {
    fn eq(&self, other: &Self) -> bool {
        (&*self.base, &self.field) == (&*other.base, &other.field)
    }
}

impl Hash for FieldExpr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&*self.base, &self.field).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct AddrOf {
    pub base: Box<Expr>,
    pub addr_type: Type,
    pub position: Position,
}

impl fmt::Display for AddrOf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "&({})", &self.base)
    }
}

impl PartialEq for AddrOf {
    fn eq(&self, other: &Self) -> bool {
        (&*self.base, &self.addr_type) == (&*other.base, &other.addr_type)
    }
}

impl Hash for AddrOf {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&*self.base, &self.addr_type).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct LabelledOld {
    pub label: String,
    pub base: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for LabelledOld {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "old[{}]({})", &self.label, &self.base)
    }
}

impl PartialEq for LabelledOld {
    fn eq(&self, other: &Self) -> bool {
        (&self.label, &*self.base) == (&other.label, &*other.base)
    }
}

impl Hash for LabelledOld {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.label, &*self.base).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct ConstExpr {
    pub value: Const,
    pub position: Position,
}

impl fmt::Display for ConstExpr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.value)
    }
}

impl PartialEq for ConstExpr {
    fn eq(&self, other: &Self) -> bool {
        &self.value == &other.value
    }
}

impl Hash for ConstExpr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.value).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct MagicWand {
    pub left: Box<Expr>,
    pub right: Box<Expr>,
    pub borrow: Option<Borrow>,
    pub position: Position,
}

impl fmt::Display for MagicWand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}) {:?} --* ({})", &self.left, &self.borrow, &self.right)
    }
}

impl PartialEq for MagicWand {
    fn eq(&self, other: &Self) -> bool {
        (&*self.left, &*self.right, self.borrow) == (&*other.left, &*other.right, other.borrow)
    }
}

impl Hash for MagicWand {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&*self.left, &*self.right, self.borrow).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct PredicateAccessPredicate {
    pub predicate_name: String,
    pub argument: Box<Expr>,
    pub permission: PermAmount,
    pub position: Position,
}

impl fmt::Display for PredicateAccessPredicate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "acc({}({}), {})", &self.predicate_name, &self.argument, self.permission)
    }
}

impl PartialEq for PredicateAccessPredicate {
    fn eq(&self, other: &Self) -> bool {
        (&self.predicate_name, &self.argument, self.permission) == (&other.predicate_name, &other.argument, other.permission)
    }
}

impl Hash for PredicateAccessPredicate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.predicate_name, &self.argument, self.permission).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct FieldAccessPredicate {
    pub base: Box<Expr>,
    pub permission: PermAmount,
    pub position: Position,
}

impl fmt::Display for FieldAccessPredicate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "acc({}, {})", &self.base, self.permission)
    }
}

impl PartialEq for FieldAccessPredicate {
    fn eq(&self, other: &Self) -> bool {
        (&*self.base, self.permission) == (&*other.base, other.permission)
    }
}

impl Hash for FieldAccessPredicate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&*self.base, self.permission).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct UnaryOp {
    pub op_kind: UnaryOpKind,
    pub argument: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({})", &self.op_kind, &self.argument)
    }
}

impl PartialEq for UnaryOp {
    fn eq(&self, other: &Self) -> bool {
        (&self.op_kind, &*self.argument) == (&other.op_kind, &*other.argument)
    }
}

impl Hash for UnaryOp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.op_kind, &*self.argument).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct BinOp {
    pub op_kind: BinOpKind,
    pub left: Box<Expr>,
    pub right: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}) {} ({})", &self.left, &self.op_kind, &self.right)
    }
}

impl PartialEq for BinOp {
    fn eq(&self, other: &Self) -> bool {
        (&self.op_kind, &*self.left, &*self.right) == (&other.op_kind, &*other.left, &*other.right)
    }
}

impl Hash for BinOp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.op_kind, &*self.left, &*self.right).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct ContainerOp {
    pub op_kind: ContainerOpKind,
    pub left: Box<Expr>,
    pub right: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for ContainerOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.op_kind {
            ContainerOpKind::SeqIndex => write!(f, "{}[{}]", &self.left, &self.right),
            ContainerOpKind::SeqConcat => write!(f, "{} ++ {}", &self.left, &self.right),
            ContainerOpKind::SeqLen => write!(f, "|{}|", &self.left),
        }
    }
}

impl PartialEq for ContainerOp {
    fn eq(&self, other: &Self) -> bool {
        (&self.op_kind, &*self.left, &*self.right) == (&other.op_kind, &*other.left, &*other.right)
    }
}

impl Hash for ContainerOp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.op_kind, &*self.left, &*self.right).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct Seq {
    pub typ: Type,
    pub elements: Vec<Expr>,
    pub position: Position,
}

impl fmt::Display for Seq {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let typ = &self.typ;
        let elems_printed = self.elements.iter().map(|e| format!("{}", e)).collect::<Vec<_>>().join(", ");
        let elem_ty = if let Type::Seq(box typ) = typ { typ } else { unreachable!() };
        write!(f, "Seq[{}]({})", elem_ty, elems_printed)
    }
}

impl PartialEq for Seq {
    fn eq(&self, other: &Self) -> bool {
        (&self.typ, &*self.elements) == (&other.typ, &*other.elements)
    }
}

impl Hash for Seq {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.typ, &*self.elements).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct Unfolding {
    pub predicate_name: String,
    pub arguments: Vec<Expr>,
    pub base: Box<Expr>,
    pub permission: PermAmount,
    pub variant: MaybeEnumVariantIndex,
    pub position: Position,
}

impl fmt::Display for Unfolding {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "(unfolding acc({}({}), {}) in {})",
            if let Some(variant_index) = &self.variant {
                format!("{}<variant {}>", &self.predicate_name, variant_index)
            } else {
                format!("{}", &self.predicate_name)
            },
            &(self.arguments).iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            self.permission,
            &self.base
        )
    }
}

impl PartialEq for Unfolding {
    fn eq(&self, other: &Self) -> bool {
        (&self.predicate_name, &self.arguments, &*self.base, self.permission, &self.variant) == (&other.predicate_name, &other.arguments, &*other.base, other.permission, &other.variant)
    }
}

impl Hash for Unfolding {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.predicate_name, &self.arguments, &*self.base, self.permission, &self.variant).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct Cond {
    pub guard: Box<Expr>,
    pub then_expr : Box<Expr>,
    pub else_expr: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for Cond {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({})?({}):({})", &self.guard, &self.then_expr, &self.else_expr)
    }
}

impl PartialEq for Cond {
    fn eq(&self, other: &Self) -> bool {
        (&*self.guard, &*self.then_expr, &*self.else_expr) == (&*other.guard, &*other.then_expr, &*other.else_expr)
    }
}

impl Hash for Cond {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&*self.guard, &*self.then_expr, &*self.else_expr).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct ForAll {
    pub variables: Vec<LocalVar>,
    pub triggers: Vec<Trigger>,
    pub body: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for ForAll {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "forall {} {} :: {}",
            (&self.variables).iter()
                .map(|x| format!("{:?}", x))
                .collect::<Vec<String>>()
                .join(", "),
            (&self.triggers)
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            (&self.body).to_string()
        )
    }
}

impl PartialEq for ForAll {
    fn eq(&self, other: &Self) -> bool {
        (&self.variables, &self.triggers, &*self.body) == (&other.variables, &other.triggers, &*other.body)
    }
}

impl Hash for ForAll {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.variables, &self.triggers, &*self.body).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct Exists {
    pub variables: Vec<LocalVar>,
    pub triggers: Vec<Trigger>,
    pub body: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for Exists {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "exists {} {} :: {}",
            (&self.variables).iter()
                .map(|x| format!("{:?}", x))
                .collect::<Vec<String>>()
                .join(", "),
            (&self.triggers)
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            (&self.body).to_string()
        )
    }
}

impl PartialEq for Exists {
    fn eq(&self, other: &Self) -> bool {
        (&self.variables, &self.triggers, &*self.body) == (&other.variables, &other.triggers, &*other.body)
    }
}

impl Hash for Exists {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.variables, &self.triggers, &*self.body).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct LetExpr {
    pub variable: LocalVar,
    pub def: Box<Expr>,
    pub body: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for LetExpr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "(let {:?} == ({}) in {})",
            &self.variable,
            (&self.def).to_string(),
            (&self.body).to_string()
        )
    }
}

impl PartialEq for LetExpr {
    fn eq(&self, other: &Self) -> bool {
        (&self.variable, &*self.def, &*self.body) == (&other.variable, &*other.def, &*other.body)
    }
}

impl Hash for LetExpr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.variable, &*self.def, &*self.body).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct FuncApp {
    pub function_name: String,
    pub arguments: Vec<Expr>,
    pub formal_arguments: Vec<LocalVar>,
    pub return_type: Type,
    pub position: Position,
}

impl fmt::Display for FuncApp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}<{},{}>({})",
            &self.function_name,
            (&self.formal_arguments)
                .iter()
                .map(|p| p.typ.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            &(self.return_type).to_string(),
            &(self.arguments).iter()
                .map(|f| f.to_string())
                .collect::<Vec<String>>()
                .join(", "),
        )
    }
}

impl PartialEq for FuncApp {
    fn eq(&self, other: &Self) -> bool {
        (&self.function_name, &self.arguments) == (&other.function_name, &other.arguments)
    }
}

impl Hash for FuncApp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.function_name, &self.arguments).hash(state);
    }
}


#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct DomainFuncApp {
    pub domain_function: DomainFunc,
    pub arguments: Vec<Expr>,
    pub position: Position,
}

impl fmt::Display for DomainFuncApp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}({})",
            (&self.domain_function).name,
            (&self.arguments).iter()
                .map(|f| f.to_string())
                .collect::<Vec<String>>()
                .join(", "),
        )
    }
}

impl PartialEq for DomainFuncApp {
    fn eq(&self, other: &Self) -> bool {
        (&self.domain_function, &self.arguments) == (&other.domain_function, &other.arguments)
    }
}

impl Hash for DomainFuncApp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&self.domain_function, &self.arguments).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct InhaleExhale {
    pub inhale_expr: Box<Expr>,
    pub exhale_expr: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for InhaleExhale {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[({}), ({})]", &self.inhale_expr, &self.exhale_expr)
    }
}

impl PartialEq for InhaleExhale {
    fn eq(&self, other: &Self) -> bool {
        (&*self.inhale_expr, &*self.exhale_expr) == (&*other.inhale_expr, &*other.exhale_expr)
    }
}

impl Hash for InhaleExhale {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&*self.inhale_expr, &*self.exhale_expr).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct Downcast {
    pub base: Box<Expr>,
    pub enum_place: Box<Expr>,
    pub field: Field,
}

impl fmt::Display for Downcast {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "(downcast {} to {} in {})",
            (&self.enum_place).to_string(),
            &self.field,
            (&self.base).to_string(),
        )
    }
}

impl PartialEq for Downcast {
    fn eq(&self, other: &Self) -> bool {
        (&*self.base, &*self.enum_place, &self.field) == (&*other.base, &*other.enum_place, &other.field)
    }
}

impl Hash for Downcast {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&*self.base, &*self.enum_place, &self.field).hash(state);
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct SnapApp {
    pub base: Box<Expr>,
    pub position: Position,
}

impl fmt::Display for SnapApp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "snap({})", (&*self.base).to_string())
    }
}

impl PartialEq for SnapApp {
    fn eq(&self, other: &Self) -> bool {
        &*self.base == &*other.base
    }
}

impl Hash for SnapApp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        (&*self.base).hash(state);
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
