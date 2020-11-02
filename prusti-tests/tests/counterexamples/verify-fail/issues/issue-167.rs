use prusti_contracts::*;

/* COUNTEREXAMPLE :
    fn test(n):
        n <- 10,
        result <- 10
*/


#[pure]
#[requires(true)]
#[requires(n > 0)]
#[requires(true)]
#[ensures(true)]
#[ensures(result == 5)] //~ ERROR postcondition of pure function definition might not hold
#[ensures(true)]
fn test(n: i32) -> i32 {
    n
}

fn main() {}
