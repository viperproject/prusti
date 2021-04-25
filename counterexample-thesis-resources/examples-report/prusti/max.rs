use prusti_contracts::*;
fn main(){}

#[ensures(result > x || result > y)]
fn max(x: i32, y: i32) -> i32 {
    if x > y {
        x
    } else {
        y
    }
}
