// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// This module is full of functions matching `Expr`s and reassembling them with one field changed
// (e.g. new position), so we feel it's ok to use single-digit identifiers there
#![allow(clippy::many_single_char_names)]

use super::super::borrows::Borrow;
use crate::vir::ast::*;

impl Expr {
    /// Apply the closure to all places in the expression.
    pub fn fold_places<F>(self, f: F) -> Expr
        where
            F: Fn(Expr) -> Expr,
    {
        struct PlaceFolder<F>
            where
                F: Fn(Expr) -> Expr,
        {
            f: F,
        }
        impl<F> ExprFolder for PlaceFolder<F>
            where
                F: Fn(Expr) -> Expr,
        {
            fn fold(&mut self, e: Expr) -> Expr {
                if e.is_place() {
                    (self.f)(e)
                } else {
                    default_fold_expr(self, e)
                }
            }
            // TODO: Handle triggers?
        }
        PlaceFolder { f }.fold(self)
    }

    /// Apply the closure to all expressions.
    pub fn fold_expr<F>(self, f: F) -> Expr
        where
            F: Fn(Expr) -> Expr,
    {
        struct ExprFolderImpl<F>
            where
                F: Fn(Expr) -> Expr,
        {
            f: F,
        }
        impl<F> ExprFolder for ExprFolderImpl<F>
            where
                F: Fn(Expr) -> Expr,
        {
            fn fold(&mut self, e: Expr) -> Expr {
                let new_expr = default_fold_expr(self, e);
                (self.f)(new_expr)
            }
        }
        ExprFolderImpl { f }.fold(self)
    }
}

pub trait ExprFolder: Sized {
    fn fold(&mut self, e: Expr) -> Expr {
        default_fold_expr(self, e)
    }

    fn fold_boxed(&mut self, e: Box<Expr>) -> Box<Expr> {
        box self.fold(*e)
    }

    fn fold_local(&mut self, v: LocalVar, p: Position) -> Expr {
        Expr::Local(v, p)
    }
    fn fold_variant(&mut self, base: Box<Expr>, variant: Field, p: Position) -> Expr {
        Expr::Variant(self.fold_boxed(base), variant, p)
    }
    fn fold_field(&mut self, receiver: Box<Expr>, field: Field, pos: Position) -> Expr {
        Expr::Field(self.fold_boxed(receiver), field, pos)
    }
    fn fold_addr_of(&mut self, e: Box<Expr>, t: Type, p: Position) -> Expr {
        Expr::AddrOf(self.fold_boxed(e), t, p)
    }
    fn fold_const(&mut self, x: Const, p: Position) -> Expr {
        Expr::Const(x, p)
    }
    fn fold_labelled_old(
        &mut self,
        label: String,
        body: Box<Expr>,
        pos: Position
    ) -> Expr {
        Expr::LabelledOld(label, self.fold_boxed(body), pos)
    }
    fn fold_magic_wand(
        &mut self,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        borrow: Option<Borrow>,
        pos: Position,
    ) -> Expr {
        Expr::MagicWand(self.fold_boxed(lhs), self.fold_boxed(rhs), borrow, pos)
    }
    fn fold_predicate_access_predicate(
        &mut self,
        name: String,
        arg: Box<Expr>,
        perm_amount: PermAmount,
        pos: Position,
    ) -> Expr {
        Expr::PredicateAccessPredicate(name, self.fold_boxed(arg), perm_amount, pos)
    }
    fn fold_credit_access_predicate(
        &mut self,
        name: String,
        args: Vec<Expr>,
        frac_perm_amount: FracPermAmount,
        pos: Position
    ) -> Expr {
        let FracPermAmount(left, right) = frac_perm_amount;
        let new_perm = FracPermAmount::new(
            self.fold_boxed(left),
            self.fold_boxed(right)
        );
        Expr::CreditAccessPredicate(
            name,
            args.into_iter().map(|e| self.fold(e)).collect(),
            new_perm,
            pos
        )
    }
    fn fold_field_access_predicate(
        &mut self,
        receiver: Box<Expr>,
        perm_amount: PermAmount,
        pos: Position
    ) -> Expr {
        Expr::FieldAccessPredicate(self.fold_boxed(receiver), perm_amount, pos)
    }
    fn fold_unary_op(&mut self, x: UnaryOpKind, y: Box<Expr>, p: Position) -> Expr {
        Expr::UnaryOp(x, self.fold_boxed(y), p)
    }
    fn fold_bin_op(
        &mut self,
        kind: BinOpKind,
        first: Box<Expr>,
        second: Box<Expr>,
        pos: Position
    ) -> Expr {
        Expr::BinOp(kind, self.fold_boxed(first), self.fold_boxed(second), pos)
    }
    fn fold_unfolding(
        &mut self,
        name: String,
        args: Vec<Expr>,
        expr: Box<Expr>,
        perm: PermAmount,
        variant: MaybeEnumVariantIndex,
        pos: Position,
    ) -> Expr {
        Expr::Unfolding(
            name,
            args.into_iter().map(|e| self.fold(e)).collect(),
            self.fold_boxed(expr),
            perm,
            variant,
            pos,
        )
    }
    fn fold_cond(
        &mut self,
        guard: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        pos: Position
    ) -> Expr {
        Expr::Cond(
            self.fold_boxed(guard),
            self.fold_boxed(then_expr),
            self.fold_boxed(else_expr),
            pos,
        )
    }
    fn fold_forall(
        &mut self,
        x: Vec<LocalVar>,
        y: Vec<Trigger>,
        z: Box<Expr>,
        p: Position,
    ) -> Expr {
        Expr::ForAll(x, y, self.fold_boxed(z), p)
    }
    fn fold_let_expr(
        &mut self,
        var: LocalVar,
        expr: Box<Expr>,
        body: Box<Expr>,
        pos: Position
    ) -> Expr {
        Expr::LetExpr(var, self.fold_boxed(expr), self.fold_boxed(body), pos)
    }
    fn fold_func_app(
        &mut self,
        name: String,
        args: Vec<Expr>,
        formal_args: Vec<LocalVar>,
        return_type: Type,
        pos: Position,
    ) -> Expr {
        Expr::FuncApp(
            name,
            args.into_iter().map(|e| self.fold(e)).collect(),
            formal_args,
            return_type,
            pos
        )
    }
    fn fold_domain_func_app(
        &mut self,
        func: DomainFunc,
        args: Vec<Expr>,
        pos: Position,
    ) -> Expr
    {
        Expr::DomainFuncApp(
            func,
            args.into_iter().map(|e| self.fold(e)).collect(),
            pos
        )
    }
    /* TODO
    fn fold_domain_func_app(
        &mut self,
        function_name: String,
        args: Vec<Expr>,
        formal_args: Vec<LocalVar>,
        return_type: Type,
        domain_name: String,
        pos: Position,
    ) -> Expr {
        Expr::DomainFuncApp(
            function_name,
            args.into_iter().map(|e| self.fold(e)).collect(),
            formal_args,
            return_type,
            domain_name,
            pos
        )
    }
    */
    fn fold_inhale_exhale(
        &mut self,
        inhale_expr: Box<Expr>,
        exhale_expr: Box<Expr>,
        pos: Position,
    ) -> Expr {
        Expr::InhaleExhale(
            self.fold_boxed(inhale_expr),
            self.fold_boxed(exhale_expr),
            pos
        )
    }

    fn fold_downcast(
        &mut self,
        base: Box<Expr>,
        enum_place: Box<Expr>,
        field: Field,
    ) -> Expr {
        Expr::Downcast(
            self.fold_boxed(base),
            self.fold_boxed(enum_place),
            field
        )
    }
    fn fold_snap_app(&mut self, e: Box<Expr>, p: Position) -> Expr {
        Expr::SnapApp(self.fold_boxed(e), p)
    }

    fn fold_container_op(
        &mut self,
        kind: ContainerOpKind,
        l: Box<Expr>,
        r: Box<Expr>,
        p: Position,
    ) -> Expr {
        Expr::ContainerOp(
            kind,
            self.fold_boxed(l),
            self.fold_boxed(r),
            p,
        )
    }

    fn fold_seq(&mut self, t: Type, elems: Vec<Expr>, p: Position) -> Expr {
        Expr::Seq(t, elems.into_iter().map(|e| self.fold(e)).collect(), p)
    }
}

pub fn default_fold_expr<T: ExprFolder>(this: &mut T, e: Expr) -> Expr {
    match e {
        Expr::Local(v, p) => this.fold_local(v, p),
        Expr::Variant(base, variant, p) => this.fold_variant(base, variant, p),
        Expr::Field(e, f, p) => this.fold_field(e, f, p),
        Expr::AddrOf(e, t, p) => this.fold_addr_of(e, t, p),
        Expr::Const(x, p) => this.fold_const(x, p),
        Expr::LabelledOld(x, y, p) => this.fold_labelled_old(x, y, p),
        Expr::MagicWand(x, y, b, p) => this.fold_magic_wand(x, y, b, p),
        Expr::PredicateAccessPredicate(x, y, z, p) => {
            this.fold_predicate_access_predicate(x, y, z, p)
        }
        Expr::CreditAccessPredicate(x, y, z, p) => this.fold_credit_access_predicate(x, y, z, p),
        Expr::FieldAccessPredicate(x, y, p) => this.fold_field_access_predicate(x, y, p),
        Expr::UnaryOp(x, y, p) => this.fold_unary_op(x, y, p),
        Expr::BinOp(x, y, z, p) => this.fold_bin_op(x, y, z, p),
        Expr::Unfolding(x, y, z, perm, variant, p) => {
            this.fold_unfolding(x, y, z, perm, variant, p)
        },
        Expr::Cond(x, y, z, p) => this.fold_cond(x, y, z, p),
        Expr::ForAll(x, y, z, p) => this.fold_forall(x, y, z, p),
        Expr::LetExpr(x, y, z, p) => this.fold_let_expr(x, y, z, p),
        Expr::FuncApp(x, y, z, k, p) => this.fold_func_app(x, y, z, k, p),
        Expr::DomainFuncApp(x, y, p) => this.fold_domain_func_app(x,y,p),
        // TODO Expr::DomainFuncApp(u, v, w, x, y, p) => this.fold_domain_func_app(u,v,w,x,y,p),
        Expr::InhaleExhale(x, y, p) => this.fold_inhale_exhale(x, y, p),
        Expr::Downcast(b, p, f) => this.fold_downcast(b, p, f),
        Expr::SnapApp(e, p) => this.fold_snap_app(e, p),
        Expr::ContainerOp(x, y, z, p) => this.fold_container_op(x, y, z, p),
        Expr::Seq(x, y, p) => this.fold_seq(x, y, p),
    }
}

pub trait ExprWalker: Sized {
    fn walk(&mut self, expr: &Expr) {
        default_walk_expr(self, expr);
    }

    fn walk_local_var(&mut self, _var: &LocalVar) {}

    fn walk_local(&mut self, var: &LocalVar, _pos: &Position) {
        self.walk_local_var(var);
    }
    fn walk_variant(&mut self, base: &Expr, _variant: &Field, _pos: &Position) {
        self.walk(base);
    }
    fn walk_field(&mut self, receiver: &Expr, _field: &Field, _pos: &Position) {
        self.walk(receiver);
    }
    fn walk_addr_of(&mut self, receiver: &Expr, _typ: &Type, _pos: &Position) {
        self.walk(receiver);
    }
    fn walk_const(&mut self, _const: &Const, _pos: &Position) {}
    fn walk_labelled_old(&mut self, _label: &str, body: &Expr, _pos: &Position) {
        self.walk(body);
    }
    fn walk_magic_wand(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        _borrow: &Option<Borrow>,
        _pos: &Position
    ) {
        self.walk(lhs);
        self.walk(rhs);
    }
    fn walk_predicate_access_predicate(
        &mut self,
        _name: &str,
        arg: &Expr,
        _perm_amount: PermAmount,
        _pos: &Position
    ) {
        self.walk(arg)
    }
    fn walk_credit_access_predicate(
        &mut self,
        _name: &str,
        args: &Vec<Expr>,
        frac_perm_amount: &FracPermAmount,
        _pos: &Position
    ) {
        for arg in args {
            self.walk(arg)
        }
        self.walk(frac_perm_amount.left());
        self.walk(frac_perm_amount.right());
    }
    fn walk_field_access_predicate(
        &mut self,
        receiver: &Expr,
        _perm_amount: PermAmount,
        _pos: &Position
    ) {
        self.walk(receiver)
    }
    fn walk_unary_op(&mut self, _op: UnaryOpKind, arg: &Expr, _pos: &Position) {
        self.walk(arg)
    }
    fn walk_bin_op(&mut self, _op: BinOpKind, arg1: &Expr, arg2: &Expr, _pos: &Position) {
        self.walk(arg1);
        self.walk(arg2);
    }
    fn walk_unfolding(
        &mut self,
        _name: &str,
        args: &Vec<Expr>,
        body: &Expr,
        _perm: PermAmount,
        _variant: &MaybeEnumVariantIndex,
        _pos: &Position
    ) {
        for arg in args {
            self.walk(arg);
        }
        self.walk(body);
    }
    fn walk_cond(&mut self, guard: &Expr, then_expr: &Expr, else_expr: &Expr, _pos: &Position) {
        self.walk(guard);
        self.walk(then_expr);
        self.walk(else_expr);
    }
    fn walk_forall(
        &mut self,
        vars: &Vec<LocalVar>,
        _triggers: &Vec<Trigger>,
        body: &Expr,
        _pos: &Position
    ) {
        for var in vars {
            self.walk_local_var(var);
        }
        self.walk(body);
    }
    fn walk_let_expr(&mut self, bound_var: &LocalVar, expr: &Expr, body: &Expr, _pos: &Position) {
        self.walk_local_var(bound_var);
        self.walk(expr);
        self.walk(body);
    }
    fn walk_func_app(
        &mut self,
        _name: &str,
        args: &Vec<Expr>,
        formal_args: &Vec<LocalVar>,
        _return_type: &Type,
        _pos: &Position
    ) {
        for arg in args {
            self.walk(arg)
        }
        for arg in formal_args {
            self.walk_local_var(arg);
        }
    }
    fn walk_domain_func_app(&mut self, func: &DomainFunc, args: &Vec<Expr>, _pos: &Position) {
        for arg in args {
            self.walk(arg)
        }
        for arg in &func.formal_args {
            self.walk_local_var(arg)
        }
    }
    /* TODO
    fn walk_domain_func_app(
        &mut self,
        _function_name: &String,
        args: &Vec<Expr>,
        formal_args: &Vec<LocalVar>,
        _return_type: &Type,
        _domain_name: &String,
        _pos: &Position) {
        for arg in args {
            self.walk(arg)
        }
        for arg in formal_args {
            self.walk_local_var(arg)
        }
    }
    */
    fn walk_inhale_exhale(&mut self, inhale_expr: &Expr, exhale_expr: &Expr, _pos: &Position) {
        self.walk(inhale_expr);
        self.walk(exhale_expr);
    }

    fn walk_downcast(&mut self, base: &Expr, enum_place: &Expr, _field: &Field) {
        self.walk(base);
        self.walk(enum_place);
    }
    fn walk_snap_app(&mut self, expr: &Expr, _pos: &Position) {
        self.walk(expr);
    }

    fn walk_container_op(
        &mut self,
        _kind: &ContainerOpKind,
        l: &Expr,
        r: &Expr,
        _p: &Position,
    ) {
        self.walk(l);
        self.walk(r);
    }

    fn walk_seq(&mut self, _ty: &Type, elems: &[Expr], _pos: &Position) {
        for elem in elems {
            self.walk(elem);
        }
    }
}

pub fn default_walk_expr<T: ExprWalker>(this: &mut T, e: &Expr) {
    match *e {
        Expr::Local(ref v, ref p) => this.walk_local(v, p),
        Expr::Variant(ref base, ref variant, ref p) => this.walk_variant(base, variant, p),
        Expr::Field(ref e, ref f, ref p) => this.walk_field(e, f, p),
        Expr::AddrOf(ref e, ref t, ref p) => this.walk_addr_of(e, t, p),
        Expr::Const(ref x, ref p) => this.walk_const(x, p),
        Expr::LabelledOld(ref x, ref y, ref p) => this.walk_labelled_old(x, y, p),
        Expr::MagicWand(ref x, ref y, ref b, ref p) => this.walk_magic_wand(x, y, b, p),
        Expr::PredicateAccessPredicate(ref x, ref y, z, ref p) => {
            this.walk_predicate_access_predicate(x, y, z, p)
        }
        Expr::CreditAccessPredicate(ref x, ref y, ref z, ref p) => this.walk_credit_access_predicate(x, y, z, p),
        Expr::FieldAccessPredicate(ref x, y, ref p) => this.walk_field_access_predicate(x, y, p),
        Expr::UnaryOp(x, ref y, ref p) => this.walk_unary_op(x, y, p),
        Expr::BinOp(x, ref y, ref z, ref p) => this.walk_bin_op(x, y, z, p),
        Expr::Unfolding(ref x, ref y, ref z, perm, ref variant, ref p) => {
            this.walk_unfolding(x, y, z, perm, variant, p)
        },
        Expr::Cond(ref x, ref y, ref z, ref p) => this.walk_cond(x, y, z, p),
        Expr::ForAll(ref x, ref y, ref z, ref p) => this.walk_forall(x, y, z, p),
        Expr::LetExpr(ref x, ref y, ref z, ref p) => this.walk_let_expr(x, y, z, p),
        Expr::FuncApp(ref x, ref y, ref z, ref k, ref p) => this.walk_func_app(x, y, z, k, p),
        Expr::DomainFuncApp(ref x, ref y,ref p) => this.walk_domain_func_app(x,y,p),
        // TODO Expr::DomainFuncApp(ref u, ref v, ref w, ref x, ref y,ref p) => this.walk_domain_func_app(u, v, w, x,y,p),
        Expr::InhaleExhale(ref x, ref y, ref p) => this.walk_inhale_exhale(x, y, p),
        Expr::Downcast(ref b, ref p, ref f) => this.walk_downcast(b, p, f),
        Expr::SnapApp(ref e, ref p) => this.walk_snap_app(e, p),
        Expr::ContainerOp(ref kind, ref l, ref r, ref p) => this.walk_container_op(kind, l, r, p),
        Expr::Seq(ref ty, ref elems, ref p) => this.walk_seq(ty, elems, p),
    }
}

pub trait FallibleExprFolder: Sized {
    type Error;

    fn fallible_fold(&mut self, e: Expr) -> Result<Expr, Self::Error> {
        default_fallible_fold_expr(self, e)
    }

    fn fallible_fold_boxed(&mut self, e: Box<Expr>) -> Result<Box<Expr>, Self::Error> {
        Ok(box self.fallible_fold(*e)?)
    }

    fn fallible_fold_local(&mut self, v: LocalVar, p: Position) -> Result<Expr, Self::Error> {
        Ok(Expr::Local(v, p))
    }
    fn fallible_fold_variant(&mut self, base: Box<Expr>, variant: Field, p: Position) -> Result<Expr, Self::Error> {
        Ok(Expr::Variant(self.fallible_fold_boxed(base)?, variant, p))
    }
    fn fallible_fold_field(&mut self, receiver: Box<Expr>, field: Field, pos: Position) -> Result<Expr, Self::Error> {
        Ok(Expr::Field(self.fallible_fold_boxed(receiver)?, field, pos))
    }
    fn fallible_fold_addr_of(&mut self, e: Box<Expr>, t: Type, p: Position) -> Result<Expr, Self::Error> {
        Ok(Expr::AddrOf(self.fallible_fold_boxed(e)?, t, p))
    }
    fn fallible_fold_const(&mut self, x: Const, p: Position) -> Result<Expr, Self::Error> {
        Ok(Expr::Const(x, p))
    }
    fn fallible_fold_labelled_old(
        &mut self,
        label: String,
        body: Box<Expr>,
        pos: Position
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::LabelledOld(label, self.fallible_fold_boxed(body)?, pos))
    }
    fn fallible_fold_magic_wand(
        &mut self,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        borrow: Option<Borrow>,
        pos: Position,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::MagicWand(
            self.fallible_fold_boxed(lhs)?,
            self.fallible_fold_boxed(rhs)?,
            borrow,
            pos
        ))
    }
    fn fallible_fold_predicate_access_predicate(
        &mut self,
        name: String,
        arg: Box<Expr>,
        perm_amount: PermAmount,
        pos: Position,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::PredicateAccessPredicate(name, self.fallible_fold_boxed(arg)?, perm_amount, pos))
    }
    fn fallible_fold_credit_access_predicate(
        &mut self,
        name: String,
        args: Vec<Expr>,
        frac_perm_amount: FracPermAmount,
        pos: Position,
    ) -> Result<Expr, Self::Error> {
        let FracPermAmount(left, right) = frac_perm_amount;
        let new_perm = FracPermAmount::new(
            self.fallible_fold_boxed(left)?,
            self.fallible_fold_boxed(right)?
        );
        Ok(Expr::CreditAccessPredicate(
            name,
            args.into_iter()
                .map(|e| self.fallible_fold(e))
                .collect::<Result<Vec<_>, Self::Error>>()?,
            new_perm,
            pos
        ))
    }
    fn fallible_fold_field_access_predicate(
        &mut self,
        receiver: Box<Expr>,
        perm_amount: PermAmount,
        pos: Position
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::FieldAccessPredicate(self.fallible_fold_boxed(receiver)?, perm_amount, pos))
    }
    fn fallible_fold_unary_op(
        &mut self,
        x: UnaryOpKind,
        y: Box<Expr>,
        p: Position
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::UnaryOp(x, self.fallible_fold_boxed(y)?, p))
    }
    fn fallible_fold_bin_op(
        &mut self,
        kind: BinOpKind,
        first: Box<Expr>,
        second: Box<Expr>,
        pos: Position
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::BinOp(
            kind,
            self.fallible_fold_boxed(first)?,
            self.fallible_fold_boxed(second)?,
            pos
        ))
    }
    fn fallible_fold_unfolding(
        &mut self,
        name: String,
        args: Vec<Expr>,
        expr: Box<Expr>,
        perm: PermAmount,
        variant: MaybeEnumVariantIndex,
        pos: Position,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::Unfolding(
            name,
            args.into_iter()
                .map(|e| self.fallible_fold(e))
                .collect::<Result<Vec<_>, Self::Error>>()?,
            self.fallible_fold_boxed(expr)?,
            perm,
            variant,
            pos,
        ))
    }
    fn fallible_fold_cond(
        &mut self,
        guard: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        pos: Position
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::Cond(
            self.fallible_fold_boxed(guard)?,
            self.fallible_fold_boxed(then_expr)?,
            self.fallible_fold_boxed(else_expr)?,
            pos,
        ))
    }
    fn fallible_fold_forall(
        &mut self,
        x: Vec<LocalVar>,
        y: Vec<Trigger>,
        z: Box<Expr>,
        p: Position,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::ForAll(x, y, self.fallible_fold_boxed(z)?, p))
    }
    fn fallible_fold_let_expr(
        &mut self,
        var: LocalVar,
        expr: Box<Expr>,
        body: Box<Expr>,
        pos: Position
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::LetExpr(
            var,
            self.fallible_fold_boxed(expr)?,
            self.fallible_fold_boxed(body)?,
            pos
        ))
    }
    fn fallible_fold_func_app(
        &mut self,
        name: String,
        args: Vec<Expr>,
        formal_args: Vec<LocalVar>,
        return_type: Type,
        pos: Position,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::FuncApp(
            name,
            args.into_iter()
                .map(|e| self.fallible_fold(e))
                .collect::<Result<Vec<_>, Self::Error>>()?,
            formal_args,
            return_type,
            pos
        ))
    }
    fn fallible_fold_domain_func_app(
        &mut self,
        func: DomainFunc,
        args: Vec<Expr>,
        pos: Position,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::DomainFuncApp(
            func,
            args.into_iter()
                .map(|e| self.fallible_fold(e))
                .collect::<Result<Vec<_>, Self::Error>>()?,
            pos
        ))
    }
    /* TODO
    fn fallible_fold_domain_func_app(
        &mut self,
        function_name: String,
        args: Vec<Expr>,
        formal_args: Vec<LocalVar>,
        return_type: Type,
        domain_name: String,
        pos: Position,
    ) -> Result<Expr, Self::Error> {
            Ok(Expr::DomainFuncApp(
            function_name,
            args.into_iter()
                .map(|e| self.fallible_fold(e))
                .collect::<Result<Vec<_>, Self::Error>>()?,
            formal_args,
            return_type,
            domain_name,
            pos
        ))
    }
    */
    fn fallible_fold_inhale_exhale(
        &mut self,
        inhale: Box<Expr>,
        exhale: Box<Expr>,
        pos: Position,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::InhaleExhale(
            self.fallible_fold_boxed(inhale)?,
            self.fallible_fold_boxed(exhale)?,
            pos
        ))
    }

    fn fallible_fold_downcast(
        &mut self,
        base: Box<Expr>,
        enum_expr: Box<Expr>,
        field: Field,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::Downcast(
            self.fallible_fold_boxed(base)?,
            self.fallible_fold_boxed(enum_expr)?,
            field
        ))
    }
    fn fallible_fold_snap_app(
        &mut self,
        e: Box<Expr>,
        p: Position
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::SnapApp(self.fallible_fold_boxed(e)?, p))
    }

    fn fallible_fold_container_op(
        &mut self,
        kind: ContainerOpKind,
        l: Box<Expr>,
        r: Box<Expr>,
        p: Position,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::ContainerOp(
            kind,
            self.fallible_fold_boxed(l)?,
            self.fallible_fold_boxed(r)?,
            p,
        ))
    }

    fn fallible_fold_seq(
        &mut self,
        ty: Type,
        elems: Vec<Expr>,
        p: Position,
    ) -> Result<Expr, Self::Error> {
        Ok(Expr::Seq(
            ty,
            elems.into_iter()
                .map(|e| self.fallible_fold(e))
                .collect::<Result<_, _>>()?,
            p,
        ))
    }
}

pub fn default_fallible_fold_expr<U, T: FallibleExprFolder<Error=U>>(
    this: &mut T, e: Expr
) -> Result<Expr, U> {
    match e {
        Expr::Local(v, p) => this.fallible_fold_local(v, p),
        Expr::Variant(base, variant, p) => this.fallible_fold_variant(base, variant, p),
        Expr::Field(e, f, p) => this.fallible_fold_field(e, f, p),
        Expr::AddrOf(e, t, p) => this.fallible_fold_addr_of(e, t, p),
        Expr::Const(x, p) => this.fallible_fold_const(x, p),
        Expr::LabelledOld(x, y, p) => this.fallible_fold_labelled_old(x, y, p),
        Expr::MagicWand(x, y, b, p) => this.fallible_fold_magic_wand(x, y, b, p),
        Expr::PredicateAccessPredicate(x, y, z, p) => {
            this.fallible_fold_predicate_access_predicate(x, y, z, p)
        }
        Expr::CreditAccessPredicate(x, y, z, p) => this.fallible_fold_credit_access_predicate(x, y, z, p),
        Expr::FieldAccessPredicate(x, y, p) => this.fallible_fold_field_access_predicate(x, y, p),
        Expr::UnaryOp(x, y, p) => this.fallible_fold_unary_op(x, y, p),
        Expr::BinOp(x, y, z, p) => this.fallible_fold_bin_op(x, y, z, p),
        Expr::Unfolding(x, y, z, perm, variant, p) => {
            this.fallible_fold_unfolding(x, y, z, perm, variant, p)
        },
        Expr::Cond(x, y, z, p) => this.fallible_fold_cond(x, y, z, p),
        Expr::ForAll(x, y, z, p) => this.fallible_fold_forall(x, y, z, p),
        Expr::LetExpr(x, y, z, p) => this.fallible_fold_let_expr(x, y, z, p),
        Expr::FuncApp(x, y, z, k, p) => this.fallible_fold_func_app(x, y, z, k, p),
        Expr::DomainFuncApp(x, y, p) => this.fallible_fold_domain_func_app(x,y,p),
        // TODO Expr::DomainFuncApp(u, v, w, x, y, p) => this.fallible_fold_domain_func_app(u,v,w,x,y,p),
        Expr::InhaleExhale(x, y, p) => this.fallible_fold_inhale_exhale(x,y,p),
        Expr::Downcast(b, p, f) => this.fallible_fold_downcast(b, p, f),
        Expr::SnapApp(e, p) => this.fallible_fold_snap_app(e, p),
        Expr::ContainerOp(x, y, z, p) => this.fallible_fold_container_op(x, y, z, p),
        Expr::Seq(x, y, p) => this.fallible_fold_seq(x, y, p),
    }
}
