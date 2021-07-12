// ignore-test: baz cannot be verified as non-pure

use prusti_contracts::*;

fn main() {}

#[pure]
fn foo(x: &[i32]) -> i32 {
    if x.len() > 0 {
        x[0]
    } else {
        3
    }
}

#[requires(s.len() > 0)]
fn bar(s: &[i32]) -> i32 {
    s[0]
}

#[pure]
#[requires(x.len() >= 3)]
fn baz(x: &[i32]) -> i32 {
    let y = &x[1..3];
    assert!(y.len() == 2);
    y[0]
}
