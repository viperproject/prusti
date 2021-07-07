// compile-flags: -Pprint_desugared_specs=true -Pprint_typeckd_specs=true -Pno_verify=true -Phide_uuids=true
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"
// normalize-stdout-test: "\[[a-z0-9]{4}\]::" -> "[$(CRATE_ID)]::"

/// Tests for predicate parsing and desugaring

use prusti_contracts::*;

predicate! {
    fn pred(a: bool) -> bool {
        forall(|b: bool| a == b)
    }
}

#[requires(pred(true))]
fn use_pred() {}

predicate! {
    fn forall_implication() -> bool {
        forall(|x: usize| (x != 0) ==> x*2 != 0)
    }
}

fn main() {}
