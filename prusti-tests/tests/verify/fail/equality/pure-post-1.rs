use prusti_contracts::*;

#[derive(Clone,PartialEq,Eq)]
struct A {
    i: i32,
}

#[pure] 
#[requires(_x == _y)]
#[ensures(_x == _y)]

/* COUNTEREXAMPLE : again not really relevant. Fails always. */
fn get_value(_x: A, _y: A) -> A { //~ ERROR return type of pure function does not implement Copy
    _x
}

fn main() {
}
