use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn foo(): 
        x <- 2,
        y <- false,
        return_value <- 4
        result <- 4

    fn foo2(): 
        x <- 4,
        y <- true,
        return_value <- -999
        
        
*/


#[ensures(match result { 4 => false, _ => true})] //~ ERROR postcondition might not hold
fn foo(x: i64, y: bool) -> i64 {
    let mut return_value = x * x;

    // ...code...

    if y && x == 4 {
        return_value = -999; // <-- bug
    }

    // ...more code...

    return_value
}

fn foo2(x: i64, y: bool) -> i64 {
    let mut return_value = x * x;

    // ...code...

    if y && x == 4 {
        return_value = -999; // <-- bug
    }

    // ...more code...

    assert!(return_value >= 0);  //~ ERROR the asserted expression might not hold
    return_value
}

fn main() {}
