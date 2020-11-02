use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn func():
        result <- 5
*/


#[ensures(result == 6)] //~ ERROR postcondition might not hold
fn func() -> u32{
    5
}

fn main() {

}
