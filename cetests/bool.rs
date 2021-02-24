use prusti_contracts::*;


#[ensures(result==true)]
fn foo(b: bool) -> bool {
    let x = true;
    if b {
        false
    } else {
        true
    }
}

fn main() {}