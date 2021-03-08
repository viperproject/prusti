use prusti_contracts::*;


#[ensures(!result)]
fn foo(x: char) -> bool{
    if x == 'c' {
        true
    } else {
        false
    }
}

fn main(){}


