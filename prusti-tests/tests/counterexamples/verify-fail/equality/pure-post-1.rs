use prusti_contracts::*;

#[derive(Clone,PartialEq,Eq)]
struct A {
    i: i32,
}

/* COUNTEREXAMPLES :
    fn get_value(_x, _y):
        _x <- A{i:42}
        _y <- A{i:42}
        result <- A{i:42}
*/ 

#[pure] 
#[requires(_x == _y)]
#[ensures(_x == _y)]
fn get_value(_x: A, _y: A) -> A { //~ ERROR return type of pure function does not implement Copy
    _x
}


fn main() {
}
