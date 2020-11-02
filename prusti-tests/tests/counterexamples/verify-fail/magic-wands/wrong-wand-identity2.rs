#![allow(dead_code)]

use prusti_contracts::*;

struct T {
    val: i32
}

/* COUNTEREXAMPLE : 
    identity2(x, v):
        this acutally does verify (?)

    identity4(x):
        x <- T {
            val : 42,
        }
        result <- T {
            val : 42,
        }
    identity5(): 
        (always fails pledge)
        x = T {
            val : 5,
        }
        result = T {
            val : 5,
        }
    identity_use3(): 
        old(t) <- T {val:5},
        old(y) <- T {val:5},
        old(z) <- T {val:5},
        z <- T{val:8},
        x <- T{val:8},
        v <- &x.val,
        t <- T{val:8}

    this poses a lot of questions:
    which variables do we display?
    at which points do we display their values?
    Decision I took for the above : old(_) -> value when variable is initialized
                                        _ -> value of the variable before verification failure
        
*/

#[after_expiry(x.val == before_expiry(result.val))]
fn identity(x: &mut T) -> &mut T {
    x
}

#[ensures(result.val == v)] // TODO x.val is illegal, but we crash instead of giving a proper error.
#[after_expiry(x.val == before_expiry(result.val))]
fn identity2(x: &mut T, v: i32) -> &mut T {
    x.val = v;
    x
}

#[ensures(*result == v)]
#[after_expiry(x.val == before_expiry(*result))]
fn identity3(x: &mut T, v: i32) -> &mut i32 {
    x.val = v;
    &mut x.val
}

#[after_expiry(x.val == 5)] //~ ERROR pledge
fn identity4(x: &mut T) -> &mut T {
    x
}

#[after_expiry(x.val != before_expiry(result.val))] //~ ERROR pledge
fn identity5(x: &mut T) -> &mut T {
    x
}

fn identity_use() {
    let mut t = T { val: 5 };
    let y = &mut t;
    let z = identity(y);
    z.val = 6;
    assert!(t.val == 6);
}

fn identity_use3() {
    let mut t = T { val: 5 };
    assert!(t.val == 5);
    let y = &mut t;
    assert!(y.val == 5);
    let z = identity(y);
    z.val = 6;
    let x = identity2(z, 7);
    let v = identity3(x, 8);
    assert!(*v == 8);
    assert!(t.val == 9); //~ ERROR: the asserted expression might not hold
}

fn main() {}
