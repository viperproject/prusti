extern crate prusti_contracts;
use prusti_contracts::*;


#[pure]
fn abs(x:f64) -> f64 { 
    if x < 0. { -x } else { x }
}


#[ensures(result == true)]
fn foo(x: f64) -> bool {
    abs(x) >= x
}

fn main() {}