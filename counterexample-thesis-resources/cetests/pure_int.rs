use prusti_contracts::*;

#[pure]
#[ensures(result!=42)]
fn foo(x: i32) -> i32 {
    x + 21
}

fn main() {}