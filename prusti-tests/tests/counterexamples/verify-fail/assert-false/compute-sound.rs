#![feature(nll)]

use prusti_contracts::*;

#[derive(Clone, Copy)]
struct IntBox {
    val: i32
}

#[derive(Clone, Copy)]
enum Expr {
    Sum(IntBox, IntBox),
    Constant(IntBox)
}
/* COUNTEREXAMPLE
    t0 <- IntBox{
        val:42
    },
    expr <- Expr::Constant(t0),
    expr_depth <- 1,
    simplified <- expr, 
    simplified_depth <- 1,
    value <- 42

    or any other instantiation of expr
*/
fn compute(expr: Expr) -> i32 {
    let expr_depth = match expr {
        Expr::Sum(_, _) => 2,
        Expr::Constant(_) => 1
    };

    let simplified = match expr {
        Expr::Sum(a, b) => Expr::Constant(
            IntBox {
                val: a.val + b.val
            }
        ),
        x => x
    };

    let simplified_depth = match simplified {
        Expr::Sum(_, _) => 2,
        Expr::Constant(_) => 1
    };

    // Some invariants
    debug_assert!(simplified_depth <= expr_depth);
    debug_assert!(simplified_depth == 1);

    let value = match simplified {
        Expr::Sum(_, _) => {
            unreachable!()
        },
        Expr::Constant(IntBox { val }) => val
    };

    assert!(false);  //~ ERROR the asserted expression might not hold

    value
}

fn main() {}
