use prusti_contracts::*;

/* COUNTEREXAMPLE:
    fn Percentage.get(self):
        self <- Effective{},
        result <- 101
*/

trait Percentage {
    #[ensures(result <= 100)] //~ ERROR postcondition might not hold
    fn get(&self) -> u8;
}

struct Effective {}

impl Percentage for Effective {
    fn get(&self) -> u8 { // originates here
        101
    }
}

fn main() {}
