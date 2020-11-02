use prusti_contracts::*;

pub struct S2 {
    f: u32,
}

/* COUNTEREXAMPLE:
    fn test4():
        x <- S2 {
            f:8,
        },
        y <- S2 {
            f:8,
        },
*/

pub fn test4() -> S2 {
    let x = S2 {
        f: 8,
    };
    let y = x;
    assert!(y.f == 9);  //~ ERROR the asserted expression might not hold
    y
}

fn main() {}
