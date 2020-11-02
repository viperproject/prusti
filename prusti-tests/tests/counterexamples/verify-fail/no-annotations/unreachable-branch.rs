struct IntBox {
    val: i32
}

enum Expr {
    Sum(IntBox, IntBox),
    Constant(IntBox)
}

/* COUNTEREXAMPLE : 
    expr <- Expr::Sum(IntBox {
        val : 42
    }, IntBox {
        val : 44
    }),
    simplified <- Expr::Sum(IntBox {
        val : 44
    }, IntBox {
        val : 42
    }),
    (fails for any expr = Expr::Sum(..))

*/

fn compute(expr: Expr) -> i32 {
    let simplified = match expr {
        Expr::Sum(a, b) => Expr::Sum(b, a),
        x => x
    };

    let value = match simplified {
        Expr::Sum(_, _) => {
            unreachable!()  //~ ERROR unreachable!(..) statement might be reachable
        },
        Expr::Constant(IntBox { val }) => val
    };

    value
}

fn main() {}
