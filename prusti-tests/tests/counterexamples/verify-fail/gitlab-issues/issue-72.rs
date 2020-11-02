#![allow(unused_comparisons)]

/// Issue #72: "Function precondition failure is missing identifier for position (causes panic)"
use prusti_contracts::*;

pub struct VecWrapperI32{
    v: Vec<i32>
}

/* COUNTEREXAMPLE: 
    fn test0(x):
        x <- VecWrapperI32{
            v: []
        }
*/

impl VecWrapperI32 {
    // Encoded as body-less Viper function
    #[trusted]
    #[pure]
    pub fn len(&self) -> usize {
        self.v.len()
    }

    // Encoded as body-less Viper method
    #[trusted]
    #[ensures(result.len() == length)]
    #[ensures(forall(|i: usize| (0 <= i && i < length) ==> result.lookup(i) == 0))]
    pub fn new(length: usize) -> Self {
        VecWrapperI32{ v: vec![0; length] }
    }

    // Encoded as body-less Viper function
    #[trusted]
    #[pure]
    #[requires(0 <= index && index < self.len())]
    pub fn lookup(&self, index: usize) -> i32 {
        self.v[index]
    }

    // Encoded as body-less Viper method
    #[trusted]
    #[requires(0 <= index && index < self.len())]
    #[ensures(self.lookup(old(index)) == old(value))]
    #[ensures(self.len() == old(self.len()))]
    #[ensures(forall(|i: usize| (0 <= i && i < self.len() && i != old(index)) ==> self.lookup(i) == old(self.lookup(i))))]
    pub fn store(&mut self, index: usize, value: i32) {
        self.v[index] = value;
    }
}

fn test0(x: VecWrapperI32) {
    let z = x.lookup(0); //~ ERROR precondition of pure function call might not hold
}

fn main() {

}
