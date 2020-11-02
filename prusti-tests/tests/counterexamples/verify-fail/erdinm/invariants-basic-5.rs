use prusti_contracts::*;

// postcondition (result) inhale

//#[invariant(self.value <= 100)]
struct Percentage {
    value: u8,
}


/* COUNTEREXAMPLE:  Can not actually be violated,
    form :
        x = 100,
        perc = Percentage {
            value : 101
        }

*/

impl Percentage {
    #[requires(value <= 100)]
    fn new(value: u8) -> Self {
        Percentage {
            value: value,
        }
    }
}

#[requires(x <= 100)]
fn test(x: u8) {
    let perc = Percentage::new(x);
    assert!(perc.value <= 100); //~ ERROR the asserted expression might not hold
}

fn main() {}
