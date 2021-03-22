use prusti_contracts::*;


pub struct Whatever{
    value: i32,
    other_value: i32,
    valid: bool,
}

pub fn main() {}

#[requires(x.value != x.other_value && x.value != 42)]
#[ensures(result.value!=42)]
pub fn foo(x: Whatever) -> Whatever {
    if x.valid {
        Whatever{
            value: 42,
            other_value: 43,
            valid: false,
        }
    } else {
        x
    }
}