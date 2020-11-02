/* COUNTEREXAMPLE :
    fn foo():
        empty
    fn bar():
        empty
    fn main():
        empty
*/


fn foo() {
    assert!(false);  //~ ERROR the asserted expression might not hold
}

fn bar() {
    assert!(false);  //~ ERROR the asserted expression might not hold
}

fn main() {
    assert!(false);  //~ ERROR the asserted expression might not hold
}
