/* COUNTEREXAMPLE : 
    fn foo(x):
        x <- 42
    (always fails)
*/

fn foo(x: i32) -> i32 {
    unimplemented!(); //~ ERROR unimplemented!(..) statement might be reachable
}

fn main(){}
