use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn test1(): 
        a <- 4,
        _x <- ref a 
        
        (how to present _x)
    fn test2(a):
        a <- 42,
        _x <- ref a
        (always fails)
    fn test3(): 
        old(a) <- 5,
        old(x) <- ref a,
        old(y) <- ref a,
        _b <- 5,
        _c <- 5,
        a <- 6,
        b <- 6
        (always fails)
        
    fn test4(a):
        a <- 42
        x <- ref a,
        y <- ref a,
        _b <- 42,
        _c <- 42,

    fn test5():
        a <- 5,
        x <- ref a,
        y <- ref a,
        _b <- 5,
        _c <- 5,
        a <- 6,
        _b <- 6

*/

pub fn test1() {
    let a = 4;
    let _x = &a;
    assert!(false); //~ ERROR the asserted expression might not hold
}

pub fn test2(a: u32) {
    let _x = &a;
    assert!(false); //~ ERROR the asserted expression might not hold
}

pub fn test3() {
    let mut a = 5;
    let x = &a;
    let y = &a;
    let _b = *x;
    let _c = *y;
    a = 6;
    let _b = a;
    assert!(false); //~ ERROR the asserted expression might not hold
}

/*
pub fn test3() {
    TODO: Implement without killing for now because we have not seen
    anyone to write code like this in practice.
    let mut a = 5;
    let mut b = 5;
    let mut x = &mut a;
    let y = &*x;
    let z = &*x;
    x = &mut b;
    let c = *y;
    let d = *z;
    *x = 6;
    assert!(b == 6);
    assert!(a == 5 && c == 5 && d == 5);
}
*/

pub fn test4(a: u32) {
    let x = &a;
    let y = &a;
    let _b = *x;
    let _c = *y;
    assert!(false); //~ ERROR the asserted expression might not hold
}

pub fn test5() {
    let mut a = 5;
    let x = &a;
    let y = x;
    let _b = *x;
    let _c = *y;
    a = 6;
    let _b = a;
    assert!(false); //~ ERROR the asserted expression might not hold
}

fn main() {
}
