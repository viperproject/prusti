use prusti_contracts::*;

#[pure]
#[requires(true)]
#[requires(n > 0)]
#[requires(true)]
#[ensures(true)]
#[ensures(result == 5)] //~ ERROR postcondition might not hold.
#[ensures(true)]
fn test(n: i32) -> i32 {
    n
}

fn main() {}
