// compile-flags: -Pprint_desugared_specs=true -Pprint_typeckd_specs=true -Phide_uuids=true
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"
// normalize-stdout-test: "\[[a-z0-9]{4}\]::" -> "[$(CRATE_ID)]::"

use prusti_contracts::*;

#[ensures(result == a || result == b)]
#[ensures(result >= a && result >= b)]
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

fn main() {}
