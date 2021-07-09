// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::super::borrows::Borrow;
use crate::vir::{ast::*, FloatSize::*};
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
    IsNaN,
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
    Min,
    Max,
    Implies,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContainerOpKind {
    SeqIndex,
    // more to follow if required
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FloatConst {
    FloatConst64(u64),
    FloatConst32(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Const {
    Bool(bool),
    Int(i64),
    BigInt(String),
    Float(FloatConst),
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
            UnaryOpKind::IsNaN => write!(f, "is_nan")
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
            BinOpKind::Min => write!(f, "min"),
            BinOpKind::Max => write!(f, "max"),
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
            Const::Float(FloatConst::FloatConst32(val)) => write!(f, "{}", val),
            Const::Float(FloatConst::FloatConst64(val)) => write!(f, "{}", val),
            Const::FnPtr => write!(f, "FnPtr"),
        }
    }
}

impl Expr {
    pub fn pos(&self) -> Position {
        match self {
            Expr::Local(_, p)
            | Expr::Variant(_, _, p)
            | Expr::Field(_, _, p)
            | Expr::AddrOf(_, _, p)
            | Expr::Const(_, p)
            | Expr::LabelledOld(_, _, p)
            | Expr::MagicWand(_, _, _, p)
            | Expr::PredicateAccessPredicate(_, _, _, p)
            | Expr::FieldAccessPredicate(_, _, p)
            | Expr::UnaryOp(_, _, p)
            | Expr::BinOp(_, _, _, p)
            | Expr::Unfolding(_, _, _, _, _, p)
            | Expr::Cond(_, _, _, p)
            | Expr::ForAll(_, _, _, p)
            | Expr::LetExpr(_, _, _, p)
            | Expr::FuncApp(_, _, _, _, p)
            | Expr::DomainFuncApp(_, _, p)
            | Expr::ContainerOp(_, _, _, p)
            | Expr::Seq(_, _, p)
            | Expr::SnapApp(_, p)
            | Expr::InhaleExhale(_, _, p) => *p,
            // TODO Expr::DomainFuncApp(_, _, _, _, _, p) => p,
            Expr::Downcast(box ref base, ..) => base.pos(),
        }
    }

    pub fn set_pos(self, pos: Position) -> Self {
        match self {
            Expr::Local(v, _) => Expr::Local(v, pos),
            Expr::Variant(base, variant_index, _) => Expr::Variant(base, variant_index, pos),
            Expr::Field(e, f, _) => Expr::Field(e, f, pos),
            Expr::AddrOf(e, t, _) => Expr::AddrOf(e, t, pos),
            Expr::Const(x, _) => Expr::Const(x, pos),
            Expr::LabelledOld(x, y, _) => Expr::LabelledOld(x, y, pos),
            Expr::MagicWand(x, y, b, _) => Expr::MagicWand(x, y, b, pos),
            Expr::PredicateAccessPredicate(x, y, z, _) => {
                Expr::PredicateAccessPredicate(x, y, z, pos)
            }
            Expr::FieldAccessPredicate(x, y, _) => Expr::FieldAccessPredicate(x, y, pos),
            Expr::UnaryOp(x, y, _) => Expr::UnaryOp(x, y, pos),
            Expr::BinOp(x, y, z, _) => Expr::BinOp(x, y, z, pos),
            Expr::Unfolding(x, y, z, perm, variant, _) => {
                Expr::Unfolding(x, y, z, perm, variant, pos)
            },
            Expr::Cond(x, y, z, _) => Expr::Cond(x, y, z, pos),
            Expr::ForAll(x, y, z, _) => Expr::ForAll(x, y, z, pos),
            Expr::LetExpr(x, y, z, _) => Expr::LetExpr(x, y, z, pos),
            Expr::FuncApp(x, y, z, k, _) => Expr::FuncApp(x, y, z, k, pos),
            Expr::DomainFuncApp(x,y,_) => Expr::DomainFuncApp(x,y,pos),
            // TODO Expr::DomainFuncApp(u,v, w, x, y ,_) => Expr::DomainFuncApp(u,v,w,x,y,pos),
            Expr::InhaleExhale(x, y, _) => Expr::InhaleExhale(x, y, pos),
            Expr::SnapApp(e, _) => Expr::SnapApp(e, pos),
            Expr::ContainerOp(x, y, z, _) => Expr::ContainerOp(x, y, z, pos),
            Expr::Seq(x, y, _) => Expr::Seq(x, y, pos),
            x => x,
        }
    }

    // Replace all Position::default() positions with `pos`
    pub fn set_default_pos(self, pos: Position) -> Self {
        struct DefaultPosReplacer {
            new_pos: Position,
        }
        impl ExprFolder for DefaultPosReplacer {
            fn fold(&mut self, e: Expr) -> Expr {
                let expr = default_fold_expr(self, e);
                if expr.pos().is_default() {
                    expr.set_pos(self.new_pos)
                } else {
                    expr
                }
            }
        }
        DefaultPosReplacer { new_pos: pos }.fold(self)
    }

    pub fn predicate_access_predicate<S: ToString>(name: S, place: Expr, perm: PermAmount) -> Self {
        let pos = place.pos();
        Expr::PredicateAccessPredicate(name.to_string(), box place, perm, pos)
    }

    pub fn field_access_predicate(place: Expr, perm: PermAmount) -> Self {
        let pos = place.pos();
        Expr::FieldAccessPredicate(box place, perm, pos)
    }

    pub fn pred_permission(place: Expr, perm: PermAmount) -> Option<Self> {
        place
            .typed_ref_name()
            .map(|pred_name| Expr::predicate_access_predicate(pred_name, place, perm))
    }

    pub fn acc_permission(place: Expr, perm: PermAmount) -> Self {
        Expr::FieldAccessPredicate(box place, perm, Position::default())
    }

    pub fn labelled_old(label: &str, expr: Expr) -> Self {
        Expr::LabelledOld(label.to_string(), box expr, Position::default())
    }

    pub fn not(expr: Expr) -> Self {
        Expr::UnaryOp(UnaryOpKind::Not, box expr, Position::default())
    }

    pub fn minus(expr: Expr) -> Self {
        Expr::UnaryOp(UnaryOpKind::Minus, box expr, Position::default())
    }

    pub fn is_nan(e: Expr) -> Self{
        Expr::UnaryOp(UnaryOpKind::IsNaN, box e, Position::default())
    }

    pub fn gt_cmp(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::GtCmp, box left, box right, Position::default())
    }

    pub fn ge_cmp(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::GeCmp, box left, box right, Position::default())
    }

    pub fn lt_cmp(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::LtCmp, box left, box right, Position::default())
    }

    pub fn le_cmp(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::LeCmp, box left, box right, Position::default())
    }

    pub fn eq_cmp(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::EqCmp, box left, box right, Position::default())
    }

    pub fn ne_cmp(left: Expr, right: Expr) -> Self {
        Expr::not(Expr::eq_cmp(left, right))
    }

    pub fn add(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::Add, box left, box right, Position::default())
    }

    pub fn sub(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::Sub, box left, box right, Position::default())
    }

    pub fn mul(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::Mul, box left, box right, Position::default())
    }

    pub fn div(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::Div, box left, box right, Position::default())
    }

    pub fn modulo(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::Mod, box left, box right, Position::default())
    }

    /// Encode Rust reminder. This is *not* Viper modulo.
    pub fn rem(left: Expr, right: Expr) -> Self {
        let abs_right = Expr::ite(
            Expr::ge_cmp(right.clone(), 0.into()),
            right.clone(),
            Expr::minus(right.clone()),
        );
        Expr::ite(
            Expr::or(
                Expr::ge_cmp(left.clone(), 0.into()),
                Expr::eq_cmp(Expr::modulo(left.clone(), right.clone()), 0.into()),
            ),
            // positive value or left % right == 0
            Expr::modulo(left.clone(), right.clone()),
            // negative value
            Expr::sub(Expr::modulo(left, right), abs_right),
        )
    }

    pub fn and(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::And, box left, box right, Position::default())
    }

    pub fn or(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::Or, box left, box right, Position::default())
    }

    pub fn xor(left: Expr, right: Expr) -> Self {
        Expr::not(Expr::eq_cmp(left, right))
    }

    pub fn min(e1: Expr, e2: Expr) -> Self{
        Expr::BinOp(BinOpKind::Min, box e1, box e2, Position::default())
    }

    pub fn max(e1: Expr, e2: Expr) -> Self{
        Expr::BinOp(BinOpKind::Max, box e1, box e2, Position::default())
    }

    pub fn implies(left: Expr, right: Expr) -> Self {
        Expr::BinOp(BinOpKind::Implies, box left, box right, Position::default())
    }

    pub fn forall(vars: Vec<LocalVar>, triggers: Vec<Trigger>, body: Expr) -> Self {
        assert!(!vars.is_empty(), "A quantifier must have at least one variable.");
        Expr::ForAll(vars, triggers, box body, Position::default())
    }

    pub fn ite(guard: Expr, left: Expr, right: Expr) -> Self {
        Expr::Cond(box guard, box left, box right, Position::default())
    }

    pub fn unfolding(
        pred_name: String,
        args: Vec<Expr>,
        expr: Expr,
        perm: PermAmount,
        variant: MaybeEnumVariantIndex,
    ) -> Self {
        Expr::Unfolding(pred_name, args, box expr, perm, variant, Position::default())
    }

    /// Create `unfolding T(arg) in body` where `T` is the type of `arg`.
    pub fn wrap_in_unfolding(arg: Expr, body: Expr) -> Expr {
        let type_name = arg.get_type().name();
        let pos = body.pos();
        Expr::Unfolding(type_name, vec![arg], box body, PermAmount::Read, None, pos)
    }

    pub fn func_app(
        name: String,
        args: Vec<Expr>,
        internal_args: Vec<LocalVar>,
        return_type: Type,
        pos: Position,
    ) -> Self {
        Expr::FuncApp(name, args, internal_args, return_type, pos)
    }

    pub fn domain_func_app(
        func: DomainFunc,
        args: Vec<Expr>,
    ) -> Self {
        Expr::DomainFuncApp(func, args, Position::default())
    }

    pub fn magic_wand(lhs: Expr, rhs: Expr, borrow: Option<Borrow>) -> Self {
        Expr::MagicWand(box lhs, box rhs, borrow, Position::default())
    }

    pub fn downcast(base: Expr, enum_place: Expr, variant_field: Field) -> Self {
        Expr::Downcast(box base, box enum_place, variant_field)
    }

    pub fn snap_app(expr: Expr) -> Self {
        Expr::SnapApp(box expr, Position::default())
    }

    pub fn find(&self, sub_target: &Expr) -> bool {
        pub struct ExprFinder<'a> {
            sub_target: &'a Expr,
            found: bool,
        }
        impl<'a> ExprWalker for ExprFinder<'a> {
            fn walk(&mut self, expr: &Expr) {
                if expr == self.sub_target {
                    self.found = true;
                } else {
                    default_walk_expr(self, expr)
                }
            }
        }

        let mut finder = ExprFinder {
            sub_target,
            found: false,
        };
        finder.walk(self);
        finder.found
    }

    /// Extract all predicates places mentioned in the expression whose predicates have the given
    /// permission amount.
    pub fn extract_predicate_places(&self, perm_amount: PermAmount) -> Vec<Expr> {
        pub struct PredicateFinder {
            predicates: Vec<Expr>,
            perm_amount: PermAmount,
        }
        impl ExprWalker for PredicateFinder {
            fn walk_predicate_access_predicate(
                &mut self,
                _name: &str,
                arg: &Expr,
                perm_amount: PermAmount,
                _pos: &Position
            ) {
                if perm_amount == self.perm_amount {
                    self.predicates.push(arg.clone());
                }
            }
        }

        let mut finder = PredicateFinder {
            predicates: Vec::new(),
            perm_amount: perm_amount,
        };
        finder.walk(self);
        finder.predicates
    }

    /// Split place into place components.
    pub fn explode_place(&self) -> (Expr, Vec<PlaceComponent>) {
        match self {
            Expr::Variant(ref base, ref variant, ref pos) => {
                let (base_base, mut components) = base.explode_place();
                components.push(PlaceComponent::Variant(variant.clone(), *pos));
                (base_base, components)
            }
            Expr::Field(ref base, ref field, ref pos) => {
                let (base_base, mut components) = base.explode_place();
                components.push(PlaceComponent::Field(field.clone(), *pos));
                (base_base, components)
            }
            _ => (self.clone(), vec![]),
        }
    }

    /// Reconstruct place from the place components.
    pub fn reconstruct_place(self, components: Vec<PlaceComponent>) -> Expr {
        components
            .into_iter()
            .fold(self, |acc, component| match component {
                PlaceComponent::Variant(variant, pos) => Expr::Variant(box acc, variant, pos),
                PlaceComponent::Field(field, pos) => Expr::Field(box acc, field, pos),
            })
    }

    // Methods from the old `Place` structure

    pub fn local(local: LocalVar) -> Self {
        Expr::Local(local, Position::default())
    }

    pub fn variant(self, index: &str) -> Self {
        assert!(self.is_place());
        let field_name = format!("enum_{}", index);
        let typ = self.get_type();
        let variant = Field::new(field_name, typ.clone().variant(index));
        Expr::Variant(box self, variant, Position::default())
    }

    pub fn field(self, field: Field) -> Self {
        Expr::Field(box self, field, Position::default())
    }

    pub fn addr_of(self) -> Self {
        let type_name = self.get_type().name();
        Expr::AddrOf(box self, Type::TypedRef(type_name), Position::default())
    }

    pub fn is_only_permissions(&self) -> bool {
        match self {
            Expr::PredicateAccessPredicate(..) |
            Expr::FieldAccessPredicate(..) => true,
            Expr::BinOp(BinOpKind::And, box lhs, box rhs, _) => {
                lhs.is_only_permissions() && rhs.is_only_permissions()
            }
            _ => false,
        }
    }

    pub fn is_place(&self) -> bool {
        match self {
            &Expr::Local(_, _) => true,
            &Expr::Variant(ref base, _, _)
            | &Expr::Field(ref base, _, _)
            | &Expr::AddrOf(ref base, _, _)
            | &Expr::LabelledOld(_, ref base, _)
            | &Expr::Unfolding(_, _, ref base, _, _, _) => base.is_place(),
            _ => false,
        }
    }

    pub fn is_variant(&self) -> bool {
        matches!(self, Expr::Variant(..))
    }

    pub fn is_call(&self) -> bool {
        matches!(self, Expr::FuncApp(..) | Expr::DomainFuncApp(..))
    }

    /// How many parts this place has? Used for ordering places.
    pub fn place_depth(&self) -> u32 {
        match self {
            &Expr::Local(_, _) => 1,
            &Expr::Variant(ref base, _, _)
            | &Expr::Field(ref base, _, _)
            | &Expr::AddrOf(ref base, _, _)
            | &Expr::LabelledOld(_, ref base, _)
            | &Expr::Unfolding(_, _, ref base, _, _, _) => base.place_depth() + 1,
            x => unreachable!("{:?}", x),
        }
    }

    pub fn is_simple_place(&self) -> bool {
        match self {
            &Expr::Local(_, _) => true,
            &Expr::Variant(ref base, _, _) | &Expr::Field(ref base, _, _) => base.is_simple_place(),
            _ => false,
        }
    }

    /// Only defined for places
    pub fn get_parent_ref(&self) -> Option<&Expr> {
        debug_assert!(self.is_place());
        match self {
            &Expr::Local(_, _) => None,
            &Expr::Variant(box ref base, _, _)
            | &Expr::Field(box ref base, _, _)
            | &Expr::AddrOf(box ref base, _, _) => Some(base),
            &Expr::LabelledOld(_, _, _) => None,
            &Expr::Unfolding(_, _, _, _, _, _) => None,
            ref x => unreachable!("{}", x),
        }
    }

    /// Only defined for places
    pub fn get_parent(&self) -> Option<Expr> {
        self.get_parent_ref().cloned()
    }

    /// Is this place a MIR reference?
    pub fn is_mir_reference(&self) -> bool {
        debug_assert!(self.is_place());
        if let Expr::Field(box Expr::Local(LocalVar { typ, .. }, _), _, _) = self {
            if let Type::TypedRef(ref name) = typ {
                // FIXME: We should not rely on string names for detecting types.
                return name.starts_with("ref$");
            }
        }
        false
    }

    /// If self is a MIR reference, dereference it.
    pub fn try_deref(&self) -> Option<Self> {
        if let Type::TypedRef(ref predicate_name) = self.get_type() {
            // FIXME: We should not rely on string names for type conversions.
            if predicate_name.starts_with("ref$") {
                let field_predicate_name = predicate_name[4..predicate_name.len()].to_string();
                let field = Field::new("val_ref", Type::TypedRef(field_predicate_name));
                let field_place = self.clone().field(field);
                return Some(field_place);
            }
        }
        None
    }

    pub fn is_local(&self) -> bool {
        matches!(self, &Expr::Local(..))
    }

    pub fn is_addr_of(&self) -> bool {
        matches!(self, &Expr::AddrOf(..))
    }

    /// Puts an `old[label](..)` around the expression
    pub fn old<S: fmt::Display + ToString>(self, label: S) -> Self {
        match self {
            Expr::Local(..) => {
                /*
                debug!(
                    "Trying to put an old expression 'old[{}](..)' around {}, which is a local variable",
                    label,
                    self
                );
                */
                self
            }
            Expr::LabelledOld(..) => {
                /*
                debug!(
                    "Trying to put an old expression 'old[{}](..)' around {}, which already has a label",
                    label,
                    self
                );
                */
                self
            }
            _ => Expr::LabelledOld(label.to_string(), box self, Position::default()),
        }
    }

    pub fn is_old(&self) -> bool {
        self.get_label().is_some()
    }

    pub fn is_curr(&self) -> bool {
        !self.is_old()
    }

    pub fn get_place(&self) -> Option<&Expr> {
        match self {
            Expr::PredicateAccessPredicate(_, ref arg, _, _) => Some(arg),
            Expr::FieldAccessPredicate(box ref arg, _, _) => Some(arg),
            _ => None,
        }
    }

    pub fn get_perm_amount(&self) -> PermAmount {
        match self {
            Expr::PredicateAccessPredicate(_, _, perm_amount, _) => *perm_amount,
            Expr::FieldAccessPredicate(_, perm_amount, _) => *perm_amount,
            x => unreachable!("{}", x),
        }
    }

    pub fn is_pure(&self) -> bool {
        struct PurityFinder {
            non_pure: bool,
        }
        impl ExprWalker for PurityFinder {
            fn walk_predicate_access_predicate(
                &mut self,
                _name: &str,
                _arg: &Expr,
                _perm_amount: PermAmount,
                _pos: &Position
            ) {
                self.non_pure = true;
            }
            fn walk_field_access_predicate(
                &mut self,
                _receiver: &Expr,
                _perm_amount: PermAmount,
                _pos: &Position
            ) {
                self.non_pure = true;
            }
        }
        let mut walker = PurityFinder { non_pure: false };
        walker.walk(self);
        !walker.non_pure
    }

    /// Remove access predicates.
    pub fn purify(self) -> Self {
        struct Purifier;
        impl ExprFolder for Purifier {
            fn fold_predicate_access_predicate(
                &mut self,
                _name: String,
                _arg: Box<Expr>,
                _perm_amount: PermAmount,
                _pos: Position
            ) -> Expr {
                true.into()
            }
            fn fold_field_access_predicate(
                &mut self,
                _receiver: Box<Expr>,
                _perm_amount: PermAmount,
                _pos: Position
            ) -> Expr {
                true.into()
            }
        }
        Purifier.fold(self)
    }

    pub fn is_heap_dependent(&self) -> bool {
        struct HeapAccessFinder {
            non_pure: bool,
        }
        impl ExprWalker for HeapAccessFinder {
            fn walk_predicate_access_predicate(
                &mut self,
                _name: &str,
                _arg: &Expr,
                _perm_amount: PermAmount,
                _pos: &Position
            ) {
                self.non_pure = true;
            }
            fn walk_field_access_predicate(
                &mut self,
                _receiver: &Expr,
                _perm_amount: PermAmount,
                _pos: &Position
            ) {
                self.non_pure = true;
            }
            fn walk_field(
                &mut self,
                _receiver: &Expr,
                _field: &Field,
                _pos: &Position
            ) {
                self.non_pure = true;
            }
            fn walk_variant(
                &mut self,
                _base: &Expr,
                _variant: &Field,
                _pos: &Position
            ) {
                self.non_pure = true;
            }
            fn walk_magic_wand(
                &mut self,
                _lhs: &Expr,
                _rhs: &Expr,
                _borrow: &Option<Borrow>,
                _pos: &Position
            ) {
                self.non_pure = true;
            }
            fn walk_unfolding(
                &mut self,
                _name: &str,
                _args: &Vec<Expr>,
                _body: &Expr,
                _perm: PermAmount,
                _variant: &MaybeEnumVariantIndex,
                _pos: &Position
            ) {
                self.non_pure = true;
            }
            fn walk_func_app(
                &mut self,
                _name: &str,
                _args: &Vec<Expr>,
                _formal_args: &Vec<LocalVar>,
                _return_type: &Type,
                _pos: &Position
            ) {
                self.non_pure = true;
            }
        }
        let mut walker = HeapAccessFinder { non_pure: false };
        walker.walk(self);
        walker.non_pure
    }

    /// Only defined for places
    pub fn get_base(&self) -> LocalVar {
        debug_assert!(self.is_place());
        match self {
            Expr::Local(ref var, _) => var.clone(),
            Expr::LabelledOld(_, ref base, _) |
            Expr::Unfolding(_, _, ref base, _, _, _) => {
                base.get_base()
            }
            _ => self.get_parent().unwrap().get_base(),
        }
    }

    pub fn get_label(&self) -> Option<&String> {
        match self {
            Expr::LabelledOld(ref label, _, _) => Some(label),
            _ => None,
        }
    }

    /* Moved to the Eq impl
    /// Place equality after type elision
    pub fn weak_eq(&self, other: &Expr) -> bool {
        debug_assert!(self.is_place());
        debug_assert!(other.is_place());
        match (self, other) {
            (
                Expr::Local(ref self_var),
                Expr::Local(ref other_var)
            ) => self_var.weak_eq(other_var),
            (
                Expr::Field(box ref self_base, ref self_field),
                Expr::Field(box ref other_base, ref other_field)
            ) => self_field.weak_eq(other_field) && self_base.weak_eq(other_base),
            (
                Expr::AddrOf(box ref self_base, ref self_typ),
                Expr::AddrOf(box ref other_base, ref other_typ)
            ) => self_typ.weak_eq(other_typ) && self_base.weak_eq(other_base),
            (
                Expr::LabelledOld(ref self_label, box ref self_base),
                Expr::LabelledOld(ref other_label, box ref other_base)
            ) => self_label == other_label && self_base.weak_eq(other_base),
            (
                Expr::Unfolding(ref self_name, ref self_args, box ref self_base, self_frac),
                Expr::Unfolding(ref other_name, ref other_args, box ref other_base, other_frac)
            ) => self_name == other_name && self_frac == other_frac &&
                self_args[0].weak_eq(&other_args[0]) && self_base.weak_eq(other_base),
            _ => false
        }
    }
    */

    pub fn has_proper_prefix(&self, other: &Expr) -> bool {
        debug_assert!(self.is_place(), "self={} other={}", self, other);
        debug_assert!(other.is_place(), "self={} other={}", self, other);
        self != other && self.has_prefix(other)
    }

    pub fn has_prefix(&self, other: &Expr) -> bool {
        debug_assert!(self.is_place());
        debug_assert!(other.is_place());
        if self == other {
            true
        } else {
            match self.get_parent() {
                Some(parent) => parent.has_prefix(other),
                None => false,
            }
        }
    }

    pub fn all_proper_prefixes(&self) -> Vec<Expr> {
        debug_assert!(self.is_place());
        match self.get_parent() {
            Some(parent) => parent.all_prefixes(),
            None => vec![],
        }
    }

    // Returns all prefixes, from the shortest to the longest
    pub fn all_prefixes(&self) -> Vec<Expr> {
        debug_assert!(self.is_place());
        let mut res = self.all_proper_prefixes();
        res.push(self.clone());
        res
    }

    /// Returns the type of the expression.
    /// For function applications, the return type is provided.
    pub fn get_type(&self) -> &Type {
        lazy_static! {
            static ref FN_PTR_TYPE: Type = Type::TypedRef("FnPtr".to_string());
        }
        match self {
            Expr::Local(LocalVar { ref typ, .. }, _)
            | Expr::Variant(_, Field { ref typ, .. }, _)
            | Expr::Field(_, Field { ref typ, .. }, _)
            | Expr::AddrOf(_, ref typ, _)
            | Expr::LetExpr(LocalVar { ref typ, ..}, _, _, _) => {
                &typ
            },
            Expr::LabelledOld(_, box ref base, _)
            | Expr::Unfolding(_, _, box ref base, _, _, _)
            | Expr::UnaryOp(_, box ref base, _) => {
                base.get_type()
            },
            Expr::FuncApp(_, _, _, ref typ, _) => {
                &typ
            },
            Expr::DomainFuncApp(ref func, _, _) => {
                &func.return_type
            },
            Expr::Const(constant, ..) => {
                match constant {
                    Const::Bool(..) => &Type::Bool,
                    Const::Int(..) | Const::BigInt(..) => &Type::Int,
                    Const::Float(FloatConst::FloatConst32(_)) => &Type::Float(F32),
                    Const::Float(FloatConst::FloatConst64(_)) => &Type::Float(F64),
                    Const::FnPtr => &FN_PTR_TYPE,
                }
            }
            Expr::BinOp(ref kind, box ref base1, box ref base2, _pos) => {
                match kind {
                    BinOpKind::EqCmp |
                    BinOpKind::NeCmp |
                    BinOpKind::GtCmp |
                    BinOpKind::GeCmp |
                    BinOpKind::LtCmp |
                    BinOpKind::LeCmp |
                    BinOpKind::And |
                    BinOpKind::Or |
                    BinOpKind::Implies => { &Type::Bool },
                    BinOpKind::Add |
                    BinOpKind::Sub |
                    BinOpKind::Mul |
                    BinOpKind::Div |
                    BinOpKind::Mod |
                    BinOpKind::Min |
                    BinOpKind::Max => {
                        let typ1 = base1.get_type();
                        let typ2 = base2.get_type();
                        assert_eq!(typ1, typ2, "expr: {:?}", self);
                        typ1
                    }
                }
            }
            Expr::Cond(_, box ref base1, box ref base2, _pos) => {
                let typ1 = base1.get_type();
                let typ2 = base2.get_type();
                assert_eq!(typ1, typ2, "expr: {:?}", self);
                typ1
            }
            Expr::ForAll(..) => {
                &Type::Bool
            }
            Expr::MagicWand(..) |
            Expr::PredicateAccessPredicate(..) |
            Expr::FieldAccessPredicate(..) |
            Expr::InhaleExhale(..) => {
                unreachable!("Unexpected expression: {:?}", self);
            }
            Expr::Downcast(box ref base, ..) => base.get_type(),
            // Note: SnapApp returns the same type as the wrapped expression,
            // to allow for e.g. field access without special considerations.
            // SnapApps are replaced later in the encoder.
            Expr::SnapApp(box ref expr, _) => {
                expr.get_type()
            }
            Expr::ContainerOp(op_kind, box ref left, box ref right, _) => {
                todo!("get_type container_op({:?}, {}, {})", op_kind, left, right)
            }
            Expr::Seq(ref ty, ..) => ty,
        }
    }

    /// If returns true, then the expression is guaranteed to be boolean. However, it may return
    /// false even for boolean expressions.
    pub fn is_bool(&self) -> bool {
        if self.is_place() {
            self.get_type() == &Type::Bool
        } else {
            match self {
                Expr::Const(Const::Bool(_), _) |
                Expr::UnaryOp(UnaryOpKind::Not, _, _) |
                Expr::FuncApp(_, _, _, Type::Bool, _) |
                Expr::ForAll(..) => {
                    true
                },
                Expr::BinOp(kind, _, _, _) => {
                    use self::BinOpKind::*;
                    *kind == EqCmp || *kind == NeCmp ||
                    *kind == GtCmp || *kind == GeCmp || *kind == LtCmp || *kind == LeCmp ||
                    *kind == And || *kind == Or || *kind == Implies
                },
                _ => {
                    false
                }
            }
        }
    }

    pub fn typed_ref_name(&self) -> Option<String> {
        match self.get_type() {
            &Type::TypedRef(ref name) => Some(name.clone()),
            _ => None,
        }
    }

    pub fn negate(self) -> Self {
        if let Expr::UnaryOp(UnaryOpKind::Not, box inner_expr, _pos) = self {
            inner_expr
        } else {
            Expr::not(self)
        }
    }

    pub fn map_labels<F>(self, f: F) -> Self
    where
        F: Fn(String) -> Option<String>,
    {
        struct OldLabelReplacer<T: Fn(String) -> Option<String>> {
            f: T,
        }
        impl<T: Fn(String) -> Option<String>> ExprFolder for OldLabelReplacer<T> {
            fn fold_labelled_old(&mut self, label: String, base: Box<Expr>, pos: Position) -> Expr {
                match (self.f)(label) {
                    Some(new_label) => base.old(new_label).set_pos(pos),
                    None => *base,
                }
            }
        }
        OldLabelReplacer { f }.fold(self)
    }

    pub fn replace_place(self, target: &Expr, replacement: &Expr) -> Self {
        // TODO: disabled for snapshot patching
        /*
        debug_assert!(target.is_place());
        assert_eq!(target.get_type(), replacement.get_type());
        if replacement.is_place() {
            assert!(
                // for copy types references are replaced by snapshots
                target.get_type() == replacement.get_type()
                    || replacement.get_type().is_snapshot(),
                "Cannot substitute '{}' with '{}', because they have incompatible types '{}' and '{}'",
                target,
                replacement,
                target.get_type(),
                replacement.get_type()
            );
        }
        */

        struct PlaceReplacer<'a> {
            target: &'a Expr,
            replacement: &'a Expr,
            // FIXME: this is a hack to support generics. See issue #187.
            //  When a less-generic function-under-test desugars specs from
            //  a more-generic function, the vir::Expr contains Local's with __TYPARAM__s,
            //  but Field's with the function-under-test's concrete types. The purpose is
            //  to "fix" the (Viper) predicates of the fields, i.e. replace those
            //  typarams with local (more) concrete types.
            typaram_substs: Option<typaram::Substs>,
            subst: bool,
        }
        impl<'a> ExprFolder for PlaceReplacer<'a> {
            fn fold(&mut self, e: Expr) -> Expr {
                if e.is_place() && &e == self.target {
                    self.subst = true;
                    self.replacement.clone()
                } else {
                    match default_fold_expr(self, e) {
                        Expr::Field(expr, mut field, pos) => {
                            if let Some(ts) = &self.typaram_substs {
                                if self.subst && field.typ.is_ref() {
                                    let inner1 = field.typ.name();
                                    let inner2 = ts.apply(&inner1);
                                    debug!("replacing:\n{}\n{}\n========", &inner1, &inner2);
                                    field = Field::new(field.name, Type::TypedRef(inner2));
                                }
                            }
                            Expr::Field(expr, field, pos)
                        }
                        x => {
                            self.subst = false;
                            x
                        }
                    }
                }
            }

            fn fold_forall(
                &mut self,
                vars: Vec<LocalVar>,
                triggers: Vec<Trigger>,
                body: Box<Expr>,
                pos: Position,
            ) -> Expr {
                if vars.contains(&self.target.get_base()) {
                    // Do nothing
                    Expr::ForAll(vars, triggers, body, pos)
                } else {
                    Expr::ForAll(
                        vars,
                        triggers
                            .into_iter()
                            .map(|x| x.replace_place(self.target, self.replacement))
                            .collect(),
                        self.fold_boxed(body),
                        pos,
                    )
                }
            }
        }
        let typaram_substs = match (&target, &replacement) {
            (Expr::Local(tv, _), Expr::Local(rv, _)) => {
                if tv.typ.is_ref() && rv.typ.is_ref() {
                    debug!(
                        "learning:\n{}\n{}\n=======",
                        &target.local_type(),
                        replacement.local_type()
                    );
                    Some(typaram::Substs::learn(
                        &target.local_type(),
                        &replacement.local_type(),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };
        PlaceReplacer {
            target,
            replacement,
            typaram_substs,
            subst: false,
        }
            .fold(self)
    }

    pub fn replace_multiple_places(self, replacements: &[(Expr, Expr)]) -> Self {
        // TODO: disabled for snapshot patching
        /*
        for (src, dst) in replacements {
            debug_assert!(src.is_place() && dst.is_place());
            if dst.is_place() {
                assert!(
                    // for copy types references are replaced by snapshots
                    src.get_type() == dst.get_type() || dst.get_type().is_snapshot(),
                    "Cannot substitute '{}' with '{}', because they have incompatible \
                    types '{}' and '{}'",
                    src,
                    dst,
                    src.get_type(),
                    dst.get_type()
                );
            }
        }
        */

        struct PlaceReplacer<'a> {
            replacements: &'a [(Expr, Expr)],
            // FIXME: this is a hack to support generics. See issue #187.
            //  When a less-generic function-under-test desugars specs from
            //  a more-generic function, the vir::Expr contains Local's with __TYPARAM__s,
            //  but Field's with the function-under-test's concrete types. The purpose is
            //  to "fix" the (Viper) predicates of the fields, i.e. replace those
            //  typarams with local (more) concrete types.
            typaram_substs: Vec<Option<typaram::Substs>>,
        }
        impl<'a> ExprFolder for PlaceReplacer<'a> {
            fn fold(&mut self, e: Expr) -> Expr {
                // Check if this matches a substitution.
                if e.is_place() {
                    let substitution = self.replacements.iter()
                        .find(|(src, _)| src == &e);
                    if let Some((_src, dst)) = substitution {
                        return dst.clone();
                    }
                }

                // Otherwise, keep folding
                default_fold_expr(self, e)
            }

            fn fold_field(&mut self, receiver: Box<Expr>, field: Field, pos: Position) -> Expr {
                // Check if the base matches a substitution.
                let base_substitution = if field.typ.is_ref() && receiver.is_place() {
                    self.replacements.iter()
                        .position(|(src, _)| src == &receiver.get_base().into())
                } else {
                    None
                };

                let new_receiver = self.fold_boxed(receiver);

                // Apply the substitution
                let new_field = if let Some(subst_index) = base_substitution {
                    assert!(field.typ.is_ref());
                    if let Some(ts) = &self.typaram_substs[subst_index] {
                        let inner1 = field.typ.name();
                        let inner2 = ts.apply(&inner1);
                        debug!("replacing:\n{}\n{}\n========", &inner1, &inner2);
                        Field::new(field.name, Type::TypedRef(inner2))
                    } else {
                        field
                    }
                } else {
                    field
                };
                Expr::Field(new_receiver, new_field, pos)
            }

            fn fold_forall(
                &mut self,
                vars: Vec<LocalVar>,
                triggers: Vec<Trigger>,
                body: Box<Expr>,
                pos: Position,
            ) -> Expr {
                // TODO: the correct solution is the following:
                // (1) skip replacements where `src` uses a quantified variable;
                // (2) rename with a fresh name the quantified variables that conflict with `dst`.
                for (src, dst) in self.replacements.iter() {
                    if vars.contains(&src.get_base()) || vars.contains(&dst.get_base()) {
                        unimplemented!(
                            "replace_multiple_places doesn't handle replacements that conflict \
                            with quantified variables"
                        )
                    }
                }

                Expr::ForAll(
                    vars,
                    triggers
                        .into_iter()
                        .map(|x| x.replace_multiple_places(self.replacements))
                        .collect(),
                    self.fold_boxed(body),
                    pos,
                )
            }
        }
        let typaram_substs = replacements.iter().map(
            |(target, replacement)| {
                match (target, replacement) {
                    (Expr::Local(tv, _), Expr::Local(rv, _)) => {
                        if tv.typ.is_ref() && rv.typ.is_ref() {
                            debug!(
                                "learning:\n{}\n{}\n=======",
                                &target.local_type(),
                                replacement.local_type()
                            );
                            Some(typaram::Substs::learn(
                                &target.local_type(),
                                &replacement.local_type(),
                            ))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
        ).collect();
        PlaceReplacer {
            replacements,
            typaram_substs,
        }.fold(self)
    }

    /// Replaces expressions like `old[l5](old[l5](_9.val_ref).foo.bar)`
    /// into `old[l5](_9.val_ref.foo.bar)`
    pub fn remove_redundant_old(self) -> Self {
        struct RedundantOldRemover {
            current_label: Option<String>,
        }
        impl ExprFolder for RedundantOldRemover {
            fn fold_labelled_old(&mut self, label: String, base: Box<Expr>, pos: Position) -> Expr {
                let old_current_label = mem::replace(&mut self.current_label, Some(label.clone()));
                let new_base = default_fold_expr(self, *base);
                let new_expr = if Some(label.clone()) == old_current_label {
                    new_base
                } else {
                    new_base.old(label).set_pos(pos)
                };
                self.current_label = old_current_label;
                new_expr
            }
        }
        RedundantOldRemover {
            current_label: None,
        }
        .fold(self)
    }

    /// Leaves a conjunction of `acc(..)` expressions
    pub fn filter_perm_conjunction(self) -> Self {
        struct PermConjunctionFilter();
        impl ExprFolder for PermConjunctionFilter {
            fn fold(&mut self, e: Expr) -> Expr {
                match e {
                    f @ Expr::PredicateAccessPredicate(..) => f,
                    f @ Expr::FieldAccessPredicate(..) => f,
                    Expr::BinOp(BinOpKind::And, y, z, p) => {
                        self.fold_bin_op(BinOpKind::And, y, z, p)
                    }

                    Expr::BinOp(..)
                    | Expr::MagicWand(..)
                    | Expr::Unfolding(..)
                    | Expr::Cond(..)
                    | Expr::UnaryOp(..)
                    | Expr::Const(..)
                    | Expr::Local(..)
                    | Expr::Variant(..)
                    | Expr::Field(..)
                    | Expr::AddrOf(..)
                    | Expr::LabelledOld(..)
                    | Expr::ForAll(..)
                    | Expr::LetExpr(..)
                    | Expr::FuncApp(..)
                    | Expr::DomainFuncApp(..)
                    | Expr::InhaleExhale(..)
                    | Expr::Downcast(..)
                    | Expr::ContainerOp(..)
                    | Expr::Seq(..)
                    | Expr::SnapApp(..) => true.into(),
                }
            }
        }
        PermConjunctionFilter().fold(self)
    }

    pub fn local_type(&self) -> String {
        match &self {
            Expr::Local(localvar, _) => match &localvar.typ {
                Type::TypedRef(str) => str.clone(),
                _ => panic!("expected Type::TypedRef"),
            },
            _ => panic!("expected Expr::Local"),
        }
    }

    /// Compute the permissions that are needed for this expression to
    /// be successfully evaluated. This is method is used for `fold` and
    /// `exhale` statements inside `package` statements because Silicon
    /// fails to compute which permissions it should take into the magic
    /// wand.
    pub fn compute_footprint(&self, perm_amount: PermAmount) -> Vec<Expr> {
        struct Collector {
            perm_amount: PermAmount,
            perms: Vec<Expr>,
        }
        impl ExprWalker for Collector {
            fn walk_variant(&mut self, e: &Expr, v: &Field, p: &Position) {
                self.walk(e);
                let expr = Expr::Variant(box e.clone(), v.clone(), *p);
                let perm = Expr::acc_permission(expr, self.perm_amount);
                self.perms.push(perm);
            }
            fn walk_field(&mut self, e: &Expr, f: &Field, p: &Position) {
                self.walk(e);
                let expr = Expr::Field(box e.clone(), f.clone(), *p);
                let perm = Expr::acc_permission(expr, self.perm_amount);
                self.perms.push(perm);
            }
            fn walk_labelled_old(&mut self, _label: &str, _expr: &Expr, _pos: &Position) {
                // Stop recursion.
            }
        }
        let mut collector = Collector {
            perm_amount: perm_amount,
            perms: Vec::new(),
        };
        collector.walk(self);
        collector.perms
    }

    /// Replace all generic types with their instantiations by using string substitution.
    /// FIXME: this is a hack to support generics. See issue #187.
    pub fn patch_types(self, substs: &HashMap<String, String>) -> Self {
        struct TypePatcher<'a> {
            substs: &'a HashMap<String, String>,
        }
        impl<'a> ExprFolder for TypePatcher<'a> {
            fn fold_predicate_access_predicate(
                &mut self,
                mut predicate_name: String,
                arg: Box<Expr>,
                perm_amount: PermAmount,
                pos: Position,
            ) -> Expr {
                for (typ, subst) in self.substs {
                    predicate_name = predicate_name.replace(typ, subst);
                }
                Expr::PredicateAccessPredicate(
                    predicate_name,
                    self.fold_boxed(arg),
                    perm_amount,
                    pos,
                )
            }
            fn fold_local(&mut self, mut var: LocalVar, pos: Position) -> Expr {
                var.typ = var.typ.patch(self.substs);
                Expr::Local(var, pos)
            }
            fn fold_func_app(
                &mut self,
                name: String,
                args: Vec<Expr>,
                formal_args: Vec<LocalVar>,
                return_type: Type,
                pos: Position,
            ) -> Expr {
                let formal_args = formal_args
                    .into_iter()
                    .map(|mut var| {
                        var.typ = var.typ.patch(self.substs);
                        var
                    })
                    .collect();
                // FIXME: We do not patch the return_type because pure functions cannot return
                // generic values.
                Expr::FuncApp(
                    name,
                    args.into_iter().map(|e| self.fold(e)).collect(),
                    formal_args,
                    return_type,
                    pos,
                )
            }
        }
        let mut patcher = TypePatcher { substs: substs };
        patcher.fold(self)
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

impl Expr {
    /// Remove read permissions. For example, if the expression is
    /// `acc(x.f, read) && acc(P(x.f), write)`, then after the
    /// transformation it will be: `acc(P(x.f), write)`.
    pub fn remove_read_permissions(self) -> Self {
        struct ReadPermRemover {}
        impl ExprFolder for ReadPermRemover {
            fn fold_predicate_access_predicate(
                &mut self,
                name: String,
                arg: Box<Expr>,
                perm_amount: PermAmount,
                p: Position,
            ) -> Expr {
                assert!(perm_amount.is_valid_for_specs());
                match perm_amount {
                    PermAmount::Write => Expr::PredicateAccessPredicate(name, arg, perm_amount, p),
                    PermAmount::Read => true.into(),
                    _ => unreachable!(),
                }
            }
            fn fold_field_access_predicate(
                &mut self,
                reference: Box<Expr>,
                perm_amount: PermAmount,
                p: Position,
            ) -> Expr {
                assert!(perm_amount.is_valid_for_specs());
                match perm_amount {
                    PermAmount::Write => Expr::FieldAccessPredicate(reference, perm_amount, p),
                    PermAmount::Read => true.into(),
                    _ => unreachable!(),
                }
            }
        }
        let mut remover = ReadPermRemover {};
        remover.fold(self)
    }
}

pub trait ExprIterator {
    /// Conjoin a sequence of expressions into a single expression.
    /// Returns true if the sequence has no elements.
    fn conjoin(&mut self) -> Expr;

    /// Disjoin a sequence of expressions into a single expression.
    /// Returns true if the sequence has no elements.
    fn disjoin(&mut self) -> Expr;
}

impl<T> ExprIterator for T
where
    T: Iterator<Item = Expr>,
{
    fn conjoin(&mut self) -> Expr {
        fn rfold<T>(s: &mut T) -> Expr
        where
            T: Iterator<Item = Expr>,
        {
            if let Some(conjunct) = s.next() {
                Expr::and(conjunct, rfold(s))
            } else {
                true.into()
            }
        }
        rfold(self)
    }

    fn disjoin(&mut self) -> Expr {
        fn rfold<T>(s: &mut T) -> Expr
        where
            T: Iterator<Item = Expr>,
        {
            if let Some(conjunct) = s.next() {
                Expr::or(conjunct, rfold(s))
            } else {
                false.into()
            }
        }
        rfold(self)
    }
}
