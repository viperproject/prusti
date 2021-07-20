// compile-flags: -Pprint_desugared_specs=true -Pprint_typeckd_specs=true -Pno_verify=true -Phide_uuids=true
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"

use prusti_contracts::*;

#[ensures(result == (forall(|i: usize| i < x.len() ==> f(x[i]))))]
fn all_of(x: &Vec<u32>, f: fn (u32) -> bool) -> bool {
    for i in x {
        if !f(*i) {
            return false;
        }
    }
    true
}