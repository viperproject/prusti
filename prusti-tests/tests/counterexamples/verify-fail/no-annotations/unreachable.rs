/* COUNTERXAMPLE :
    fn foo():
        x <- 42
    (always fails)
*/

fn foo(x: i32) -> i32 {
    unreachable!(); //~ ERROR unreachable!(..) statement might be reachable
}

fn main(){}
