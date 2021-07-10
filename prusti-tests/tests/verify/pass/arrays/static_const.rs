use prusti_contracts::*;

#[ensures(result == 1)]
const fn foo() -> usize {
    1
}

fn main() {
    let bar: [usize; foo()] = [1];
    assert!(bar[0] == foo());
}
