#![allow(dead_code)]

use prusti_contracts::*;

struct T {
    val: i32
}

#[pledge(after_unblocked(x.val) == before_expiry(result.val))]
fn identity(x: &mut T) -> &mut T {
    x
}

#[ensures(result.val == v)] // TODO x.val is illegal, but we crash instead of giving a proper error.
#[pledge(after_unblocked(x.val) == before_expiry(result.val))]
fn identity2(x: &mut T, v: i32) -> &mut T {
    x.val = v;
    x
}

#[ensures(*result == v)]
#[pledge(after_unblocked(x.val) == before_expiry(*result))]
fn identity3(x: &mut T, v: i32) -> &mut i32 {
    x.val = v;
    &mut x.val
}

#[pledge(after_unblocked(x.val) == 5)]
//~^ ERROR pledge in the postcondition might not hold.
fn identity4(x: &mut T) -> &mut T {
    x
}

#[pledge(after_unblocked(x.val) != before_expiry(result.val))]
//~^ ERROR pledge in the postcondition might not hold.
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

fn main() {}
