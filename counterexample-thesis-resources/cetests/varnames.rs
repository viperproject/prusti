use prusti_contracts::*;

#[ensures(result != 3)] 
fn foo(a: i32, b:i32) -> i32 {
    if a == 42 {
        let b = 3;
        return b
    } else {
        let a = 46;
        return a
    }
}


fn main(){}