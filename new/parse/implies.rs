// compile-flags: -Zprint-desugared-specs -Zprint-typeckd-specs
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"

use prusti_contracts::*;

#[requires(true && true)]
fn test1() {}

#[ensures(true)]
fn test2() {}

fn test3() {
    for _ in 0..2 {
        invariant!(true)
    }
}

#[requires(true)]
#[ensures(true)]
fn test4() {
    for _ in 0..2 {
        invariant!(true)
    }
}

fn main() {}
