#![feature(register_tool)]
#![register_tool(prusti)]

use prusti_contracts::*;

struct Point { x: u32, y: u32 }

#[pledge(after_unblocked(p.x) == before_expiry(*result))]
#[pledge(after_unblocked(p.y) == 0)]
fn f0(p: &mut Point) -> &mut u32 {
    p.y = 0;
    &mut p.x
}

#[pledge(after_unblocked(p.x) == before_expiry(*result.0))]
#[pledge(after_unblocked(q.x) == before_expiry(*result.1))]
#[pledge(after_unblocked(p.y) == after_unblocked(q.y))]
fn f1<'p, 'q>(p: &'p mut Point, q: &'q mut Point) -> (&'p mut u32, &'q mut u32) {
    p.y = q.y;
    (&mut p.x, &mut q.x)
}

#[pledge(b ==> after_unblocked(q.x) == before_expiry(*result.1))]
#[pledge(!b ==> after_unblocked(p.y) == before_expiry(*result.1))]
#[pledge(after_unblocked(p.x) == before_expiry(*result.0))]
fn f2<'a: 'b, 'b>(p: &'a mut Point, q: &'b mut Point, b: bool) -> (&'a mut u32, &'b mut u32) {
    let x1 = &mut p.x;
    let x2 = if b { &mut q.x } else { &mut p.y };
    (x1, x2)
}

#[pledge(after_unblocked(p.x) == before_expiry(*result.0))]
#[pledge(after_unblocked(p.y) == before_expiry(*result.1))]
fn f3<'a: 'b, 'b>(p: &'a mut Point) -> (&'a mut u32, &mut u32) {
    (&mut p.x, &mut p.y)
}

#[pledge(after_unblocked(a.x) == before_expiry(*result.0))]
#[pledge(after_unblocked(b.x) == before_expiry(*result.1))]
#[pledge(after_unblocked(c.x) == before_expiry(*result.2))]
fn f4<'a: 'd, 'b: 'd + 'e, 'c: 'f, 'd, 'e, 'f>(
    a: &'a mut Point,
    b: &'b mut Point,
    c: &'c mut Point
) -> (&'d mut u32, &'e mut u32, &'f mut u32) {
    (&mut a.x, &mut b.x, &mut c.x)
}

#[pledge(old(a.x) > 0 ==> after_unblocked(a.y) == before_expiry(*result.0))]
#[pledge(old(a.x) <= 0 ==> after_unblocked(b.y) == before_expiry(*result.0))]
#[pledge(after_unblocked(b.x) == before_expiry(*result.1))]
#[pledge(after_unblocked(c.x) == before_expiry(*result.2))]
fn f5<'a: 'd, 'b: 'd + 'e, 'c: 'f, 'd, 'e, 'f>(
    a: &'a mut Point,
    b: &'b mut Point,
    c: &'c mut Point
) -> (&'d mut u32, &'e mut u32, &'f mut u32) {
    let t0 = if a.x > 0 {
        &mut a.y
    } else {
        &mut b.y
    };
    (t0, &mut b.x, &mut c.x)
}

fn main() {}
