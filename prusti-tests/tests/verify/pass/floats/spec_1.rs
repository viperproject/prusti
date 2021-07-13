extern crate prusti_contracts;
use prusti_contracts::*;

#[requires(x > 0.)]
#[ensures(result == true)]
fn id(x: f64) -> bool {
    x > 0.
}

fn main() {}