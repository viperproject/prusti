// ignore-test: foo is not supported as impure method

use prusti_contracts::*;

struct Foo {
    x: usize,
    y: usize,
}

fn main() {}

// This testcase (also) tests that in function name mangling, if the element type of a sequence is
// itself a snapshot (like for the array snapshot constructor for `f` here) we do the recursive
// call to encode the element type's name as well. if we didn't, it would come out as something
// like Seq$Snapshot(elem_ty) and later stages will choke on the parentheses.
#[pure]
fn foo(f: [Foo; 1]) -> usize {
    f[0].x + f[0].y
}
