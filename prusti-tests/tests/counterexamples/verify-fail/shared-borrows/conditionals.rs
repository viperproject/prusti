use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn test1(a, b, cond): 
        a <- 42,
        b <- 43,
        cond <- false,
        x <- ref b,
        (always fails)

    fn test1_1(a, b, cond): 
        a <- 42,
        b <- 43,
        cond <- true,
        x <- ref a,

    test1_2(a, b, cond): 
        a <- 42,
        b <- 43,
        cond <- false,
        x <- ref b,

    test2(a, b, cond): 
        a <- 42,
        b <- 43,
        cond <- true,
        x <- ref a,
        _y <- ref a
        (always fails)

    test3(cond):
        cond <- true,
        old(a) <- 5,
        old(b) <- 6,
        x <- ref a,
        a <- 7,
        a <- 8,
        (always fails)

    test3_1(cond) : 
        cond <- true,
        a <- 5,
        b <- 6,
        x <- ref a,

    test3_2(cond) : 
        cond <- false,
        a <- 5,
        b <- 6,
        x <- ref b,
        

*/

fn borrow(_x: &u32) {}

pub fn test1(a: u32, b: u32, cond: bool) {
    let x;
    if cond {
        x = &a;
    } else {
        x = &b;
    }
    borrow(x);
    assert!(false); //~ ERROR the asserted expression might not hold
}

pub fn test1_1(a: u32, b: u32, cond: bool) {
    let x;
    if cond {
        x = &a;
        assert!(false); //~ ERROR the asserted expression might not hold
    } else {
        x = &b;
    }
    borrow(x);
}

pub fn test1_2(a: u32, b: u32, cond: bool) {
    let x;
    if cond {
        x = &a;
    } else {
        x = &b;
        assert!(false); //~ ERROR the asserted expression might not hold
    }
    borrow(x);
}

pub fn test2(a: u32, b: u32, cond: bool) {
    let x;
    if cond {
        x = &a;
    } else {
        x = &b;
    }
    let _y = x;
    assert!(false); //~ ERROR the asserted expression might not hold
}

pub fn test3(cond: bool) {
    let mut a = 5;
    let mut b = 6;
    let x;
    if cond {
        x = &a;
    } else {
        x = &b;
    }
    borrow(x);
    if cond {
        assert!(*x == 5);
    } else {
        assert!(*x == 6);
    }
    a = 7;
    b = 8;
    assert!(a == 7 && b == 8);
    assert!(false); //~ ERROR the asserted expression might not hold
}

pub fn test3_1(cond: bool) {
    let mut a = 5;
    let mut b = 6;
    let x;
    if cond {
        x = &a;
    } else {
        x = &b;
    }
    borrow(x);
    if cond {
        assert!(*x == 5);
        assert!(false); //~ ERROR the asserted expression might not hold
    } else {
        assert!(*x == 6);
    }
    a = 7;
    b = 8;
    assert!(a == 7 && b == 8);
}

pub fn test3_2(cond: bool) {
    let mut a = 5;
    let mut b = 6;
    let x;
    if cond {
        x = &a;
    } else {
        x = &b;
    }
    borrow(x);
    if cond {
        assert!(*x == 5);
    } else {
        assert!(*x == 6);
        assert!(false); //~ ERROR the asserted expression might not hold
    }
    a = 7;
    b = 8;
    assert!(a == 7 && b == 8);
}

fn main() {
}
