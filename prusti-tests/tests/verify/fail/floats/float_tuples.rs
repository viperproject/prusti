extern crate prusti_contracts;
use prusti_contracts::*;

#[ensures(result == true)] //~ ERROR postcondition might not hold.
fn foo(t: (f64,f64)) -> bool {
    t.0 > t.1 || t.0 <= t.1 
}

fn main() {}