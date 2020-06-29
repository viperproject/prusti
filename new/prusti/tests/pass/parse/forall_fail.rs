// compile-flags: -Zprint-desugared-specs -Zprint-typeckd-specs -Zskip-verify -Zhide-uuids
// normalize-stdout-test: "[a-z0-9]{32}" -> "$(NUM_UUID)"
// normalize-stdout-test: "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}" -> "$(UUID)"

#![feature(register_tool)]
#![register_tool(prusti)]

use prusti_contracts::*;

#[requires(forall)]
fn test1() {}

#[requires(forall())]
fn test2() {}

#[requires(forall(|))]
fn test3() {}

#[requires(forall(||) 1+1)]
fn test4() {}

#[requires(forall(|a, b| true))]
fn test5() {}

#[requires(forall(||) || forall(||))]
fn test6() {}

#[requires(forall(|| 1+1 == 1+1, triggers=[1]))]
fn test7() {}

#[requires(forall(|| true, triggers=[(1,2), 1]))]
fn test8() {}

#[requires(forall(|| true, triggers=1))]
fn test9() {}

#[requires(forall(||))]
fn test10() {}

#[requires(forall(|| 1+1 == 1+1))]
fn test11() {}

#[requires(forall(||, triggers=[]))]
fn test12() {}

#[requires(forall(|| 1+1 == 1+1, triggers=[(1,)]))]
fn test13() {}

fn main() {}
