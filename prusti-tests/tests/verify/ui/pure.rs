// compile-flags: -Pprint_desugared_specs=true -Pprint_typeckd_specs=true -Phide_uuids=true
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"
// normalize-stdout-test: "\[[a-z0-9]{4}\]::" -> "[$(CRATE_ID)]::"

use prusti_contracts::*;

#[pure]
fn identity(x: u32) -> u32 { x }

fn test_identity1() {
    assert!(5 == identity(5));
}

#[ensures(6 == identity(6))]
fn test_identity2() {
}

#[pure]
fn max(a: i32, b: i32) -> i32 {
    if a > b {
        a
    } else {
        b
    }
}

fn test_max1() {
    let a = 5;
    let b = 6;
    let z = max(a, b);
    assert!(z == 6);
}

fn test_max2() {
    let a = 5;
    let b = 6;
    let z = max(a, b);
    assert!(z == 5);
}

#[ensures(

    true && (true ==>

        result == 3     // test that we get correct span information
            )

    && (true || false)

        )

    ]
fn test_max3() -> i32 {
    let a = 4;
    let b = 3;
    max(a, b)
}

#[requires(a > b)]
#[ensures(result == max(a, b))]
fn test_max4(a: i32, b: i32) -> i32 {
    a
}

#[requires(a < b)]
#[ensures(result == max(a, b))]
fn test_max5(a: i32, b: i32) -> i32 {
    a
}

fn main() {}
