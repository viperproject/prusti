use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn identity(x): 
        x <- 42,
        result <- 43
*/

#[ensures(result == old(x))] //~ ERROR postcondition might not hold
fn identity(x: i32) -> i32 {
    x + 1
}

fn main() {

}
