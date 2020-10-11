#![allow(warnings)]

use prusti_contracts::*;

struct Point { x: u32, y: u32 }

fn f0(p: &mut Point) -> &mut u32 {
    p.y = 0;
    &mut p.x
}

fn f0_caller_0() {
    let mut p = Point { x: 0, y: 0 };
    let px = f0(&mut p);
    *px += 1;
}

fn f0_caller_1() {
    let mut p = Point { x: 0, y: 0 };
    let px = if p.x > 0 {
        f0(&mut p)
    } else {
        &mut p.x
    };
    *px += 1;
}

#[pledge(after_unblocked(p.x) == before_expiry(*result))]
#[pledge(after_unblocked(p.y) == 0)]
#[ensures(*result == old(p.x))]
fn f1(p: &mut Point) -> &mut u32 {
    p.y = 0;
    &mut p.x
}

fn f1_caller() {
    let mut p = Point { x: 0, y: 0 };
    let px = f1(&mut p);
    *px += 1;
    assert!(p.x == 1);
}

fn f2(p: &mut Point) -> (&mut u32, &mut u32) {
    (&mut p.x, &mut p.y)
}

fn f2_caller() {
    let mut p = Point { x: 0, y: 0 };
    let (px, py) = f2(&mut p);
    *px += 1;
    *py += 2;
}

fn f3<'p, 'q>(p: &'p mut Point, q: &'q mut Point) -> (&'p mut u32, &'q mut u32) {
    (&mut p.x, &mut q.x)
}

fn f3_caller() {
    let mut p = Point { x: 10, y: 20 };
    let mut q = Point { x: 30, y: 40 };
    let (px, qx) = f3(&mut p, &mut q);
    *px += 1;
    *qx += 2;
}

#[pledge(after_unblocked(p.x) == before_expiry(*result.0))]
#[ensures(*result.0 == old(p.x))]
fn f4<'p, 'q>(p: &'p mut Point, q: &'q mut Point) -> (&'p mut u32, &'q mut u32) {
    (&mut p.x, &mut q.x)
}

fn f4_caller_0() {
    let mut p = Point { x: 10, y: 20 };
    let mut q = Point { x: 30, y: 40 };
    let (px, _) = f4(&mut p, &mut q);
    *px += 1;
    // TODO: This does not verify due to the way output references of function calls are handled.
    //  First of all, when the magic wand for the call to f4 is applied, the pledges inside it
    //  don't use the value of px returned by the function call, but instead the current value of
    //  px. This is necessary because an assignment like "*px = *x" does not change the memory
    //  location that px is pointing to, but in Viper this is encoded as "px.val_ref = x.val_ref",
    //  which does change the memory location px is pointing to. Because we'd like to assert after
    //  the assignment "*px = *x" that all the places aliased by px now also hold the value *x, we
    //  must use the current value of px.val_ref in the magic wand, not the one from right after
    //  the call.
    //  //
    //  Now this would work fine as long as Rust doesn't move references around. Consider what is
    //  happening when we call f4 like this:
    //      let (px, _) = f4(...);
    //  The MIR models this as two statements (slightly simplified):
    //      _1 = f4(...);
    //      _2 = move _1.0;
    //  In Viper, this is encoded like this:
    //      _1 = f4(...);
    //      _2.val_ref = _1.tuple_0.val_ref;
    //  And later on, when we overwrite *_2, Viper generates the following statement:
    //      _2.val_ref = ...;
    //  Finally, we expire _1.0 and would like to use the current value in the pledges. Instead of
    //  using the value of _1.tuple_0.val_ref from right after the function call, we use the
    //  current value instead. The problem is that because _1.0 was moved into _2 earlier, we
    //  actually have to use the current value of _2, not the current value of _1.0.
    // assert!(p.x == 11);
}

fn f4_caller_1() {
    let mut p = Point { x: 10, y: 20 };
    let mut q = Point { x: 30, y: 40 };
    let (px, _) = f4(&mut p, &mut q);
    *px = 1;
    assert!(p.x == 1);
}

// fn f5<'p, 'q>(p: &'p mut Point, q: &'q mut Point) -> (&'p mut u32, &'q mut u32) {
//     (&mut p.x, &mut q.x)
// }

// TODO: This test still fails because of process_expire_borrows.
// fn f5_caller() {
//     let mut p = Point { x: 10, y: 20 };
//     let mut q = Point { x: 30, y: 40 };
//     f5(&mut p, &mut q);
// }

// fn f6<'p, 'q: 'p>(p: &'p mut Point, q: &'q mut Point) -> (&'p mut u32, &'q mut u32) {
//     (&mut p.x, &mut q.x)
// }

// TODO: This test still fails because of process_expire_borrows.
// fn f6_caller() {
//     let mut p = Point { x: 10, y: 20 };
//     let mut q = Point { x: 30, y: 40 };
//     f6(&mut p, &mut q);
// }

#[pledge(b ==> after_unblocked(q.x) == before_expiry(*result.1))]
#[pledge(!b ==> after_unblocked(p.y) == before_expiry(*result.1))]
#[pledge(after_unblocked(p.x) == before_expiry(*result.0))]
fn f7<'a: 'b, 'b>(p: &'a mut Point, q: &'b mut Point, b: bool) -> (&'a mut u32, &'b mut u32) {
    let x1 = &mut p.x;
    let x2 = if b { &mut q.x } else { &mut p.y };
    (x1, x2)
}

fn f7_caller(b: bool) {
    let mut p = Point { x: 10, y: 20 };
    let mut q = Point { x: 30, y: 40 };
    let (x1, x2) = f7(&mut p, &mut q, b);
    *x1 = 11;
    *x2 = 41;
    assert!(p.x == 11);
    if b {
        assert!(q.x == 41);
    } else {
        assert!(p.y == 41);
    }
}

#[pledge(old(a.x) > 5 ==> after_unblocked(a.y) == before_expiry(*result.0))]
#[pledge(old(a.x) <= 5 ==> after_unblocked(b.y) == before_expiry(*result.0))]
#[pledge(after_unblocked(b.x) == before_expiry(*result.1))]
#[pledge(after_unblocked(c.x) == before_expiry(*result.2))]
fn f8<'a: 'd, 'b: 'd + 'e, 'c: 'f, 'd, 'e, 'f>(
    a: &'a mut Point,
    b: &'b mut Point,
    c: &'c mut Point
) -> (&'d mut u32, &'e mut u32, &'f mut u32) {
    let t0 = if a.x > 5 {
        &mut a.y
    } else {
        &mut b.y
    };
    (t0, &mut b.x, &mut c.x)
}

fn f8_caller_0() {
    let mut a = Point { x: 10, y: 20 };
    let mut b = Point { x: 30, y: 40 };
    let mut c = Point { x: 50, y: 60 };
    let (x, y, z) = f8(&mut a, &mut b, &mut c);
    *x = 21;
    assert!(a.y == 21);
}

fn f8_caller_1() {
    let mut a = Point { x: 0, y: 20 };
    let mut b = Point { x: 30, y: 40 };
    let mut c = Point { x: 50, y: 60 };
    let (x, y, z) = f8(&mut a, &mut b, &mut c);
    *x = 41;
    assert!(b.y == 41);
}

fn main() {}
