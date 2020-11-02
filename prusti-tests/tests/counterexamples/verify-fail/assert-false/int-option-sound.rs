#![feature(nll)]

use prusti_contracts::*;

/* 
    COUNTEREXAMPLE : 
        x <- IntOption::Some(42),
        y <- IntOption::Some(123),
        ret <- 42,
        
        or any instantiation of x
*/

enum IntOption {
    Some(i32),
    None
}

fn foo(x: IntOption) -> i32 {
    let y = IntOption::Some(123);
    let ret = match x {
        IntOption::Some(y) => y,
        IntOption::None => 456
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn main() {

}
