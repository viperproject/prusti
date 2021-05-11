use prusti_contracts::*;

#[requires(*x != 0)]
#[ensures(result != 14)]
fn foo(x: &i32) -> i32{
    let y = *x;
    match y {
        x => x * 2
    }
}

fn main(){}

