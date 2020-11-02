use prusti_contracts::*;


/* COUNTEREXAMPLES : 
(TODO_CE: make visible that these are pointers)
    fn test1(a):
        a <- 42,
        x <- ref a,
        _y <- ref a

    fn test2(x):
        x <- 42,
        _y <- ref x

    fn test3(x):
        x <- 42,
        y <- ref x,
        _a <- 42
    
    test4(x):
        old(x) <- 42,
        y <- ref x,
        a <- 42,
        x <- 5


*/

fn borrow(_x: &u32) {}

pub fn test1(mut a: u32) {
    let x = &mut a;
    let _y = &*x;
    assert!(false); //~ ERROR the asserted expression might not hold
}

pub fn test2(x: &mut u32) {
    let _y = &*x;
    assert!(false); //~ ERROR the asserted expression might not hold
}

pub fn test3(x: &mut u32) {
    let y = &*x;
    assert!(*y == *x);
    let _a = *y;
    assert!(false); //~ ERROR the asserted expression might not hold
}

#[ensures(*x == 5)]
pub fn test4(x: &mut u32) {
    let y = &*x;
    assert!(*y == *x);
    let a = *y;
    assert!(a == *y && a == *x);
    *x = 5;
    assert!(false); //~ ERROR the asserted expression might not hold
}

fn main() {
}
