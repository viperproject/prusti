use prusti_contracts::*;

#[requires(x.0 > 0 && x.1 =='c')]
#[ensures(result.1 >= 0)]
fn foo(x: (i32, char)) -> (char, i32) {
    let y = x.0 - 2;
    let z = x.1;
    (z, y)
}

fn main() {}
