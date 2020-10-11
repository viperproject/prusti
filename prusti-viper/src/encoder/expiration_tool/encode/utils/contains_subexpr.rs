use prusti_common::vir;
use prusti_common::vir::{ExprWalker, Expr, default_walk_expr};

pub(in super::super) fn contains_subexpr(expr: &vir::Expr, subexpr: &vir::Expr) -> bool {
    let mut walker = ContainsSubexpr { subexpr, result: false };
    walker.walk(expr);
    walker.result
}

struct ContainsSubexpr<'a> {
    subexpr: &'a vir::Expr,
    result: bool
}

impl<'a> ExprWalker for ContainsSubexpr<'a> {
    fn walk(&mut self, expr: &Expr) {
        if !self.result {
            if self.subexpr == expr {
                self.result = true;
            } else {
                default_walk_expr(self, expr);
            }
        }
    }
}
