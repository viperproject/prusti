use prusti_contracts::*;

pub enum Choose {
    One,
    Two(i32),
    Three(bool)
}

#[ensures(result==true)]
fn foo(x:Choose) -> bool {
    match x {
        Choose::One => true,
        Choose::Two(x) => x > -3,
        Choose::Three(b) => true
    }
}

fn main(){}