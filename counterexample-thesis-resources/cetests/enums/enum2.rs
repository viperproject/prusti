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

#[ensures(result)]
fn bar(x: Something, y: Something) -> bool {
    if let Something::First = x {
        true
    } else {
        if let Something::First = y {
            false
        } else {
            true
        }
    }
}

fn main() {}