/* COUNTEREXAMPLE : 
    fn foo():
        x <- false
*/


fn foo(x: bool) {
    assert!(x);  //~ ERROR the asserted expression might not hold
}

fn main() {}
