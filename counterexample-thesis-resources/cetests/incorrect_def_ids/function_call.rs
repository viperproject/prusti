use prusti_contracts::*;

#[pure]
#[requires(x >= 0)]
fn sum(x: i32) -> i32{
    if x <= 0 {
        return 0
    } else {
        return x + sum(x-1)
    }
}

#[requires(sum(-3) == 0)]
fn main(){}