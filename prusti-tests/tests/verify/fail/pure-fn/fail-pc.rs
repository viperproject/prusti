use prusti_contracts::*;

#[pure]
#[ensures(result == n*(n+1)/2)] //~ ERROR postcondition
fn sum1(n: i32) -> i32 {
    if n <= 0 { 0 } else { sum1(n-1)+n }
}

#[pure]
#[requires(n >= 0)]
#[ensures(result == n*(n+1)/2)]
fn sum2(n: i32) -> i32 {
    if n <= 0 { 0 } else { sum2(n-1)+n }
}

#[requires(sum1(-1) == 0)]
#[requires(sum2(-1) == 0)] //~ ERROR precondition
fn main() {}
