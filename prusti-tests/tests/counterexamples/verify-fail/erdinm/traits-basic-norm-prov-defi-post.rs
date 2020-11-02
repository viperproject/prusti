use prusti_contracts::*;

/* COUNTEREXAMPLE:
    fn Percentage.get(self):
        self <- ?,
        result <- 101
*/

trait Percentage {
    #[ensures(result <= 100)] //~ ERROR postcondition might not hold
    fn get(&self) -> u8 {
        101
    }
}

fn main() {}
