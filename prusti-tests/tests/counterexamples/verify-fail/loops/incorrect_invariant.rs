use prusti_contracts::*;


/* COUNTEREXAMPLE : 
    (will always fail)
    fn simple_loop():
        x <- 0
*/
pub fn simple_loop() {
    let mut x = 0;
    while x < 100 {
        body_invariant!(x == 42); //~ ERROR loop invariant might not hold
        x += 1;
    }
}

fn main() {}
