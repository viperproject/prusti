/* COUNTEREXAMPLE :
    foo():
        x <- false

*/


macro_rules! assert {
    ( $( $args:expr ),* ) => {
        panic!( $( $args ),* )  //~ ERROR panic!(..) statement might be reachable
    };
}

fn foo(x: bool) {
    assert!(x);
}

fn main() {}
