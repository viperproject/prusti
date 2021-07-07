// compile-flags: -Pprint_desugared_specs=true -Pprint_typeckd_specs=true -Phide_uuids=true
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"
// normalize-stdout-test: "\[[a-z0-9]{4}\]::" -> "[$(CRATE_ID)]::"

use prusti_contracts::*;

struct T {
    f: u32,
    g: u32,
}

fn borrows(_x: &mut u32) {
    let mut a = T { f: 1, g: 2 };
    assert!(a.f == 1);
    assert!(a.g == 2);
    let x = &mut a;
    let y = &mut x.f;
    let z = &x.g;
    *y = 5;
    assert!(*z == 2);
    assert!(a.f == 5);
}

fn borrows_fail(_x: &mut u32) {
    let mut a = T { f: 1, g: 2 };
    assert!(a.f == 1);
    assert!(a.g == 2);
    let x = &mut a;
    let y = &mut x.f;
    let z = &x.g;
    *y = 5;
    assert!(*z == 2);
    assert!(a.f == 6);
}

#[after_expiry(result => before_expiry(*result) == x.f)]
fn reborrow<'a>(x: &'a mut T) -> &'a mut u32 {
    &mut x.f
}

fn reborrow2(x: &mut T) -> &mut u32 {
    &mut x.f
}

fn reborrow_caller(a: T) {
    let mut a = a;
    let x = &mut a;
    let y = reborrow(x);
    *y = 5;
    assert!(a.f == 5);
}

fn main() {}
