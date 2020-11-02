#![feature(never_type)]

/* COUNTEREXAMPLE :
    fn diverging()
        empty, always fails

*/

fn diverging() -> ! {
    panic!();  //~ ERROR panic!(..) statement might be reachable
}

fn main() {}
