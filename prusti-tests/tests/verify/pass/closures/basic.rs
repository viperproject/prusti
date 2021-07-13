// ignore-test: need to investigate why this one fails

use prusti_contracts::*;

#[requires(add |= |a: i32, b: i32| [
    requires(a >= 0),
    requires(b >= 0),
    ensures(result == a + b)
])]
#[ensures(result == 16)]
fn test1<F: Fn (i32, i32) -> i32>(add: F) -> i32 {
    // TODO: higher-order calls cannot be encoded yet
    // add(7, 9)
    16
}

fn main() {
    let f = closure!(
        requires(i >= 0),
        ensures(result == i + 1),
        |i: i32| -> i32 { i + 1 }
    );
    f(0);

    let add = closure!(
        requires(a >= 0 && b >= 0),
        ensures(result == a + b),
        |a: i32, b: i32| -> i32 { a + b }
    );
    test1(add);
}
