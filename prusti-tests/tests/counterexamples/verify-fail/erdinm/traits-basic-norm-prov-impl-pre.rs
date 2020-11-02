use prusti_contracts::*;

/* COUNTEREXAMPLE:
    fn Effective.set(self, arg):
        self <- Effective{},
        arg <- 100
*/

trait Percentage {
    #[requires(arg <= 100)]
    fn set(&mut self, arg: u8) {
        assert!(arg <= 100);
    }
}

struct Effective {}

impl Percentage for Effective {
    fn set(&mut self, arg: u8) {
        assert!(arg <= 99); //~ ERROR the asserted expression might not hold
    }
}

fn main() {}
