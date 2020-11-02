extern crate prusti_contracts;
use prusti_contracts::*;

/* COUNTEREXAMPLE : This program actually verifies in Silicon
    so there is no way to generate one. Also would not really make sense
    (fails for pre-verification reasons -> CE_RM)
*/

struct TestStruct {}

impl TestStruct {
    #[pure]
    #[ensures(result >= a && result >= b)]
    #[ensures(result == a || result == b)]
    fn max(a: i32, b: i32) -> i32 {
        if a < b {
            b
        } else {
            a
        }
    }
}



#[extern_spec]
impl TestStruct {
    #[pure] //~ ERROR already specified function
    #[ensures(result >= a && result >= b)]
    #[ensures(result == a || result == b)]
    fn max(a: i32, b: i32) -> i32;
}

fn main() {
    let x = TestStruct::max(1, 2);
    assert!(x == 2)
}
