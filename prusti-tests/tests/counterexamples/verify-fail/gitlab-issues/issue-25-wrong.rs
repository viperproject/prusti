/// Issue #25 "Exhaling postconditions with `old(..)`"

use prusti_contracts::*;

struct T {
    f: i32,
}

/*
COUNTEREXAMPLE : 
    fn extract(x):
        x <- T {
            f:44,
        },
        y <- x
        z <- T {
            f:42,
        },
        k <- z,
        result <- 42,

*/

#[ensures(old(x.f) == result)] //~ ERROR postcondition
fn extract(x: &mut T) -> i32 {
    // move x
    let y = x;
    let mut z = T { f: 42 };
    let k = &mut z;
    k.f
    //y.f
}

fn main() {

}
