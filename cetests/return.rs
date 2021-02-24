use prusti_contracts::*;

fn main(){}

#[ensures(false)]
fn foo(x:i32) -> i32{
    if x > 0 {
        return 3
    } 
    let y = 5 + x;
    y*2
}