use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn test():
        x <- -1
*/


#[requires(x > 0)]
fn foo(x: i32) {
    // nothing
}

fn test() {
    let x = -1;
    foo(x); //~ ERROR: precondition
}

fn main() {}
