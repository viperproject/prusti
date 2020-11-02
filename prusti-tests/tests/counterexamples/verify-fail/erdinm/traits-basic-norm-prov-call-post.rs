use prusti_contracts::*;

/* COUNTEREXAMPLE:
    fn test<T:Percentage>(t):
        t <- ?,
        p <- 100,
*/

trait Percentage {
    #[ensures(result <= 100)]
    fn get(&self) -> u8 {
        100
    }
}


fn test<T: Percentage>(t: &T) {
    let p = t.get();
    assert!(p <= 99); //~ ERROR the asserted expression might not hold
}

fn main() {}
