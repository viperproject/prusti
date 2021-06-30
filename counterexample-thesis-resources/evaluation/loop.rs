use prusti_contracts::*;

#[ensures(result < 16)]
fn spurious() -> i32 {
    let mut x = 10;
    let mut y = 1;
    while(x > 0) {
        body_invariant!(x >= 0 && y > 0);
        x = x - 1;
        y = y + 1;
    }
    y
}

fn main() {}