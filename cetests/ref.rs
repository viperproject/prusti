use prusti_contracts::*;

#[requires(*x != 0)]
#[ensures(false)]
fn foo(x: &i32) -> i32{
    let mut y = *x;
    y = y * 3;
    y
}

fn main(){}

