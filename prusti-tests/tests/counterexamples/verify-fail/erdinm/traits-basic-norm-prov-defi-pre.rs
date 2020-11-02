use prusti_contracts::*;

/* COUNTEREXAMPLE:
    fn Percentage.set(self, arg):
        self <- ?,
        arg <- 100,
*/

trait Percentage {
    #[requires(arg <= 100)]
    fn set(&mut self, arg: u8) {
        assert!(arg <= 99); //~ ERROR the asserted expression might not hold
    }
}

fn main() {}
