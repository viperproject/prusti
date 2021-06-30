use prusti_contracts::*;

pub struct SomeStruct{
    value: i32,
    other_value: i32,
    valid: bool,
}

pub fn main() {}

pub fn foo(x: SomeStruct) {
    assert!(x.value == x.other_value || x.valid)
}