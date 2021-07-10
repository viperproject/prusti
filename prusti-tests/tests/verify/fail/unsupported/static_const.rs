const fn foo() -> usize {
    1
}

fn main() { //~ ERROR: [Prusti internal error] tried to encode a projection that accesses the field 0 of a variant without first downcasting its enumeration std::option::Option<std::fmt::Arguments
    let bar: [usize; foo()] = [1];
    assert_eq!(bar[0], foo());
}
