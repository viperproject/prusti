use prusti_contracts::*;


#[ensures(result > 42)]
fn foo(x: u32) -> u32{
    let y = x + 1;
    y*2
}

fn main(){}

