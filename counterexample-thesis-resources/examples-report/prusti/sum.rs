use prusti_contracts::*;

fn main(){}

#[ensures(result == n*(n+1)/2)]
fn sum(n: i32) -> i32 {
    if n <= 0 {
        0 
    } else {
        sum(n-1) + n
    }
}