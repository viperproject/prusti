use prusti_contracts::*;

struct Point { x: u32, y: u32 }

#[pledge(after_unblocked(after_unblocked(p.x)) == before_expiry(*result))]
//~^ ERROR cannot nest a before_expiry or after_unblocked environment inside another before_expiry or after_unblocked environment.
fn f00(p: &mut Point) -> &mut u32 {
    &mut p.x
}

#[pledge(after_unblocked(before_expiry(*result)) == before_expiry(*result))]
//~^ ERROR cannot nest a before_expiry or after_unblocked environment inside another before_expiry or after_unblocked environment.
fn f01(p: &mut Point) -> &mut u32 {
    &mut p.x
}

#[pledge(after_unblocked(p.x) == before_expiry(after_unblocked(p.x)))]
//~^ ERROR cannot nest a before_expiry or after_unblocked environment inside another before_expiry or after_unblocked environment.
fn f02(p: &mut Point) -> &mut u32 {
    &mut p.x
}

#[pledge(after_unblocked(p.x) == before_expiry(before_expiry(*result)))]
//~^ ERROR cannot nest a before_expiry or after_unblocked environment inside another before_expiry or after_unblocked environment.
fn f03(p: &mut Point) -> &mut u32 {
    &mut p.x
}

#[pledge(after_unblocked(*result) == before_expiry(*result))]
//~^ ERROR the after_unblocked environment cannot contain output references.
fn f10(p: &mut Point) -> &mut u32 {
    &mut p.x
}

#[pledge(after_unblocked(p.x) == before_expiry(p.x))]
//~^ ERROR the before_expiry environment cannot contain input references.
fn f11(p: &mut Point) -> &mut u32 {
    &mut p.x
}

// TODO: This should produce the error "this before_expiry environment has dependencies on multiple
//  inputs or outputs.", but it does not. The problem is that q is not blocked by something, and
//  therefore it is not picked up as a dependency of the after_unblocked context.
// #[pledge(after_unblocked(p.x + q.x) == before_expiry(*result))]
// fn f20<'p, 'q>(p: &'p mut Point, q: &'q mut Point) -> &'p mut u32 {
//     &mut p.x
// }

#[pledge(after_unblocked(p.x) == before_expiry(*result.0 + *result.1))]
//~^ ERROR this before_expiry environment has dependencies on multiple inputs or outputs.
fn f21<'p, 'q>(p: &'p mut Point, q: &'q mut Point) -> (&'p mut u32, &'q mut u32) {
    (&mut p.x, &mut q.x)
}

#[pledge(after_unblocked(0) == before_expiry(*result))]
//~^ ERROR this after_unblocked environment must contain at least one input or output reference.
fn f30(p: &mut Point) -> &mut u32 {
    &mut p.x
}

#[pledge(after_unblocked(p.x) == before_expiry(0))]
//~^ ERROR this before_expiry environment must contain at least one input or output reference.
fn f31(p: &mut Point) -> &mut u32 {
    &mut p.x
}

// This fails to verify because there are some permissions missing in a package statment. The
// culprit is the statement "unfold u32(old[pre](_1.val_ref).f$y)", which fails because there may
// be insufficient permissions to access "old[pre](_1.val_ref).f$y". This is solved by inserting an
// "assert acc(old[pre](_1.val_ref).f$y)" before the statement, which makes the necessary
// permissions available in the package statement.
// #[pledge(after_unblocked(p.y) == after_unblocked(q.y))]
// fn f1<'p, 'q>(p: &'p mut Point, q: &'q mut Point) -> (&'p mut u32, &'q mut u32) {
//     (&mut p.x, &mut q.x)
// }

fn main() {}
