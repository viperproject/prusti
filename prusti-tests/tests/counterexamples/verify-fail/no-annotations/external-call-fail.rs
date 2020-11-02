/* COUNTEREXAMPLE :
    fn test(x):
        x <- -1,
        is_pos <- false

*/

fn test(x: i32) {
    let is_pos = x.is_positive();
    assert!(is_pos); //~ ERROR the asserted expression might not hold
}

fn main(){}
