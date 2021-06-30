use prusti_contracts::*;

fn main(){}

fn foo(x: (i32, i32)) {
    assert!(x.0 == x.1)
}