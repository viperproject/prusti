// compile-flags: -Pprint_desugared_specs=true -Pprint_typeckd_specs=true -Pno_verify=true -Phide_uuids=true
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"
// normalize-stdout-test: "\[[a-z0-9]{4}\]::" -> "[$(CRATE_ID)]::"

use prusti_contracts::*;

#[requires(forall(|a: i32| forall(|a: i32| a==a)))]
fn test1() {}

#[requires(forall(|a: i32| forall(|b: i32| a==a ==> b==b)))]
fn test2() {}

#[requires(forall(|a: i32| forall(|b: i32| forall(|c: i32| a==a && b==b))))]
fn test3() {}

fn main() {}
