use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn test1(): 
        t0_get_i32 <- -1

    fn test2(): 
        t0_get_usize <- 0

    fn test3(x, y, z): 
        x <- 0,
        y <- 0,
        z <- -1

    fn test4(x): 
        x <- 0
        
    test5(x):
        x <- 5


*/

#[pure]
#[trusted]
fn get_usize() -> usize {
    unimplemented!();
}

#[pure]
#[trusted]
fn get_u32() -> u32 {
    unimplemented!();
}

#[pure]
#[trusted]
fn get_i32() -> i32 {
    unimplemented!();
}

fn test1() {
    assert!(get_usize() >= 0);
    assert!(get_u32() >= 0);
    assert!(get_i32() >= 0);    //~ ERROR the asserted expression might not hold
}

fn test2() {
    assert!(get_usize() >= 1);  //~ ERROR the asserted expression might not hold
}

fn test3(x: usize, y: u32, z: i32) {
    assert!(x >= 0);
    assert!(y >= 0);
    assert!(z >= 0);    //~ ERROR the asserted expression might not hold
}

fn test4(x: usize) {
    assert!(x >= 1);  //~ ERROR the asserted expression might not hold
}

fn test5(x: usize) {
    test4(4 - x);  //~ ERROR type invariant expected by the function call might not hold.
}

fn main() {}
