/* COUNTEREXAMPLES : 
    fn foo(x):
        x <- false
*/


macro_rules! my_assert {
    ( $( $args:expr ),* ) => {
        assert!( $( $args ),* )  //~ ERROR the asserted expression might not hold
    };
}

fn foo(x: bool) {
    my_assert!(x);
}

fn main() {}
