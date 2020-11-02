use prusti_contracts::*;

/* COUNTEREXAMPLE:
    fn test<T: Percentage>(t):
        t <- ?,
    (fails always)
*/

trait Percentage {
    #[requires(arg <= 100)]
    fn set(&mut self, arg: u8);
}

fn test<T: Percentage>(t: &mut T) {
    t.set(101); //~ ERROR precondition might not hold
}

fn main() {}
