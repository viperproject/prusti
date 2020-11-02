use prusti_contracts::*;

/* COUNTEREXAMPLE :
    fn magic(n):
        n <- 42,
        result <- 42
*/


#[pure]
#[requires(n > 0)]
#[ensures(true)]
#[ensures(true && (result == 5 || false))] //~ ERROR postcondition
#[ensures(true)]
fn magic(n: i32) -> i32 {
    n
}

fn main() {}
