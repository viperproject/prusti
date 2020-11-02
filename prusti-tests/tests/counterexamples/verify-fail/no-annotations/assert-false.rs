/* COUNTEREXAMPLE : 
    fn main():
        empty
    (always fails)
*/

fn main() {
    assert!(false);  //~ ERROR the asserted expression might not hold
}
