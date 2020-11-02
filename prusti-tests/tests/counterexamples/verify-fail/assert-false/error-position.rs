use prusti_contracts::*;

/* 
    COUNTEREXAMPLE : 
        fn foo1(x) - fn foo12(x):
            x <- true
            (or x = false)
            
        fn foo13(x): 
            x <- 42,
            result <- 42
            (or x = any integer)
*/

#[ensures(false)] //~ ERROR postcondition might not hold
fn foo1(x: bool) {}

#[ensures(false && false)] //~ ERROR postcondition might not hold
fn foo2(x: bool) {}

#[ensures(!true)] //~ ERROR postcondition might not hold
fn foo3(x: bool) {}

#[ensures(!(true || x))] //~ ERROR postcondition might not hold
fn foo4(x: bool) {}

#[ensures(!(false || true))] //~ ERROR postcondition might not hold
fn foo5(x: bool) {}

#[ensures(!(x || !false))] //~ ERROR postcondition might not hold
fn foo6(x: bool) {}

#[ensures(!(x || !x))] //~ ERROR postcondition might not hold
fn foo7(x: bool) {}

#[ensures(true ==> false)] //~ ERROR postcondition might not hold
fn foo8(x: bool) {}

#[ensures(x || true ==> !(x || !x))] //~ ERROR postcondition might not hold
fn foo9(x: bool) {}

#[ensures(x == x)]
#[ensures(false)] //~ ERROR postcondition might not hold
fn foo10(x: bool) {}

#[ensures(false)] //~ ERROR postcondition might not hold
#[ensures(x == x)]
fn foo11(x: bool) {}

#[ensures(x == x)]
#[ensures(!true)] //~ ERROR postcondition might not hold
#[ensures(x == x)]
fn foo12(x: bool) {}

#[ensures(false)] //~ ERROR postcondition might not hold
#[ensures(result == x)]
pub fn foo13(x: u32) -> u32 {
    x
}

fn main() {}
