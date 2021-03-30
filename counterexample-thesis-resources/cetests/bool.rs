use prusti_contracts::*;


#[ensures(result==true)]
fn foo(b: bool) -> bool {
    if b {
        false
    } else {
        true
    }
}

fn main() {}