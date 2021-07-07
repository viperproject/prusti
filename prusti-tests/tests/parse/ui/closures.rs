// compile-flags: -Pprint_desugared_specs=true -Pprint_typeckd_specs=true -Pno_verify=true -Phide_uuids=true
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"
// normalize-stdout-test: "\[[a-z0-9]{4}\]::" -> "[$(CRATE_ID)]::"

/// Tests for closure macro parsing.

use prusti_contracts::*;

fn main() {
    let f1 = closure!(
        requires(i >= 0),
        ensures(result == i + 1),
        |i: i32| -> i32 { i + 1 }
    );
    let f2 = closure!(
        ensures(result == i + 1),
        |i: i32| -> i32 { i + 1 }
    );
    let f3 = closure!(
        requires(i >= 0),
        |i: i32| -> i32 { i + 1 }
    );
    let f4 = closure!(
        |i: i32| -> i32 { i + 1 }
    );
    let f5 = closure!(
        || -> i32 { 1 }
    );
    let f6 = closure!(
        |i: i32, j: i32| -> i32 { i + j }
    );
    let f7 = closure!(
        |i: i32, j: i32, k: i32| -> i32 { i + j + k }
    );
}
