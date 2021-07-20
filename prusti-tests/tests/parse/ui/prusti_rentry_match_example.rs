// compile-flags: -Pprint_desugared_specs=true -Pprint_typeckd_specs=true -Pno_verify=true -Phide_uuids=true
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"

use prusti_contracts::*;

#[ensures(
    match result {
        Ok(x) => (forall(|i: usize| i < s.len() ==> is_digit(s[i]))) && int_to_string(x) == s,
        Err(_) => (forall(|i: usize| i < s.len() ==> !is_digit(s[i])))
    }
)]
fn try_string_to_int(s: &str) -> Result<u32, <u32 as std::str::FromStr>::Err> {
    s.parse::<u32>()
}