use prusti_contracts::*;

#[pure]
#[ensures(result==true)]
fn foo(b: bool) -> bool {
    if b {
        false
    } else {
        true
    }
}

fn main() {}