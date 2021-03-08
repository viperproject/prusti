use prusti_contracts::*;

pub enum Something{
    First,
    Second,
    Third
}

#[ensures(result)]
fn foo(x: Something) -> bool {
    match x {
        Something::Third => false,
        _ => true
    }
}

fn main() {}