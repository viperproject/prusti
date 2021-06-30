use prusti_contracts::*;

fn foo(x:(i32, bool)) {
    if x.0 == 32 {
        if !x.1 {
            assert!(x.0 == 0)
        }
    }
}

fn main(){}