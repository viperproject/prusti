use prusti_contracts::*;

/* COUNTEREXAMPLE :
    fn u32_foo_call_2() : 
        n <- 5,
    fn u32_foo_call_4() : 
        empty
        (one of the cases where an intermediate value would be useful!)
    fn u32_foo_call_6() : 
        empty
        (same question)
*/


#[pure]
#[requires(a + b <= std::u32::MAX)]
fn sum_pure(a: u32, b: u32) -> u32 {
    a + b
}

#[trusted]
#[pure]
fn u32_foo() -> u32 {
    5
}

fn u32_foo_call_1() {
    let n = u32_foo();
    assert!(0 <= n);
}

fn u32_foo_call_2() {
    let n = u32_foo();
    assert!(n <= 4294967295); //~ ERROR
}

fn u32_foo_call_3() {
    assert!(0 <= u32_foo());
}

fn u32_foo_call_4() {
    assert!(u32_foo() <= 4294967295); //~ ERROR
}

#[ensures(0 <= u32_foo())]
fn u32_foo_call_5() {
}

#[ensures(u32_foo() <= 4294967295)] //~ ERROR postcondition
fn u32_foo_call_6() {
}

fn main() {}
