use prusti_contracts::*;

#[pure]
#[ensures(result == false)]
fn bad(n: i32) -> bool {
    false
}

/* COUNTEREXAMPLE : 
    fn magic(n):
        n <- 10,
        result <- 10
    (always fails)
*/

#[pure]
#[requires(n > 0)]
#[ensures(true)]
#[ensures(bad(n))] //~ ERROR postcondition
#[ensures(true)]
fn magic(n: i32) -> i32 {
    n
}

fn main() {}
