use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn main(): 
        empty

*/

#[pure]
#[requires(0 <= n && n <= 2)]
fn magic(n: i32) -> i32 {
    match n {
        -1 => 123,
        0 => -1,
        1 => 1,
        2 => 42,
        _ => unreachable!()
    }
}

#[ensures(magic(-1) == 123)] //~ ERROR
#[ensures(magic(0) == -1)]
#[ensures(magic(1) == 1)]
#[ensures(magic(2) == 42)]
fn main() {}