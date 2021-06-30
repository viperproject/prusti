use prusti_contracts::*;

#[ensures(result > x + 5)]
fn incr(x: i32) -> i32 {
    x + 10
}

#[ensures(result == 20)]
fn use_inc() -> i32 {
    let x = 10;
    incr(x)
}


fn main(){}
