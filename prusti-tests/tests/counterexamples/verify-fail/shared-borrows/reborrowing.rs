use prusti_contracts::*;


/* COUNTEREXAMPLES : 
    fn reborrow(x):
        x <- 42,

    fn reborrow2(x):
        x <- 42,
        result <- 42

    fn test1():
        a <- 5,
        x <- ref a,
        y <- ref a,
        a <- 6,
        
*/

#[ensures(*result == old(*x))]
pub fn reborrow(x: &u32) -> &u32 {
    assert!(false); //~ ERROR the asserted expression might not hold
    x
}

#[ensures(false)] //~ ERROR postcondition might not hold.
#[ensures(*result == old(*x))]
pub fn reborrow2(x: &u32) -> &u32 {
    x
}

pub fn test1() {
    let mut a = 5;
    let x = &a;
    let y = reborrow(x);
    assert!(a == 5);
    assert!(*x == 5);
    assert!(*y == 5);
    assert!(a == 5);
    a = 6;
    assert!(a == 6);
    assert!(false); //~ ERROR the asserted expression might not hold
}

fn main() {
}

