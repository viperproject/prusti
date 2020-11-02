use prusti_contracts::*;

/* COUNTEREXAMPLE:
    fn Percentage.get(self):
        self <- ?,
        result <- 101

    fn Fail.set(self, arg):
        self <- Fail{},
        arg <- 100,

    fn test_get_fail<T: Percentage>(t):
        t <- ?,
        p <- 100,

    fn test_set_fail<T: Percentage>(t):
        t <- ?,
        
*/

trait Percentage {
    #[ensures(result <= 100)] //~ ERROR postcondition might not hold
    fn get(&self) -> u8;

    #[requires(arg <= 100)]
    fn set(&mut self, arg: u8);
}

struct Fail {}

impl Percentage for Fail {
    fn get(&self) -> u8 { // originates here
        101
    }
    fn set(&mut self, arg: u8) {
        assert!(arg <= 99); //~ ERROR the asserted expression might not hold
    }
}

struct Pass {}

impl Percentage for Pass {
    fn get(&self) -> u8 {
        100
    }
    fn set(&mut self, arg: u8) {
        assert!(arg <= 100);
    }
}

fn test_get_fail<T: Percentage>(t: &T) {
    let p = t.get();
    assert!(p <= 99); //~ ERROR the asserted expression might not hold
}

fn test_get_pass<T: Percentage>(t: &T) {
    let p = t.get();
    assert!(p <= 100);
}

fn test_set_fail<T: Percentage>(t: &mut T) {
    t.set(101); //~ ERROR precondition might not hold
}

fn test_set_pass<T: Percentage>(t: &mut T) {
    t.set(100);
}

fn main() {}
