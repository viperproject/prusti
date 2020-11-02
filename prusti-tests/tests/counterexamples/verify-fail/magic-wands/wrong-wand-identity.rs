#![feature(nll)]

use prusti_contracts::*;

struct T {
    val: i32
}

/* COUNTEREXAMPLES : 
    fn identity(x):
        x <- T{
            val: 42,
        },
        result <- T{
            val: 42
        }
*/
#[ensures(false)] //~ ERROR postcondition
fn identity(x: &mut T) -> &mut T {
    x
}

fn main() {}
