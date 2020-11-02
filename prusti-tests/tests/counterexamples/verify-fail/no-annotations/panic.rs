/* COUNTEREXAMPLE :
    fn main():
        empty
*/


fn main() {
    panic!();  //~ ERROR panic!(..) statement might be reachable
}
