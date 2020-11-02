use prusti_contracts::*;

/* COUNTEREXAMPLE:
    for traits it seems to be very hard to present a counterexample, for this 
    you would basically need an example of a struct implementing this trait.
    
    fn test<T: Percentage>(t):
        t: ?
*/

trait Percentage {
    #[requires(arg <= 100)]
    fn set(&mut self, arg: u8) {
        assert!(arg <= 100);
    }
}

fn test<T: Percentage>(t: &mut T) {
    t.set(123); //~ ERROR precondition might not hold
}

fn main() {}
