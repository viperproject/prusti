extern crate prusti_contracts;

#[invariant="self.d1 == self.d2"]
trait Foo { }

struct Dummy {
    d1: isize,
    d2: isize,
}

impl Foo for Dummy { }

fn test_dummy(d: &Dummy) {
    assert!(d.d1 == d.d2);
}

fn main() { }
