use prusti_contracts::*;

fn get_u32() -> u32 {
    123
}

/* COUNTEREXAMPLE : not including any variables or parameters. */

#[requires(get_u32() == 123)]
//~^ ERROR use of impure function "get_u32" in pure code
fn client_1() {}

#[requires(if false { get_u32() == 123 } else { 1 == 1 })]
//~^ ERROR use of impure function "get_u32" in pure code
fn client_2() {}

fn main() {}
