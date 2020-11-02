extern crate prusti_contracts;
use prusti_contracts::*;

#[extern_spec]
impl<T> std::vec::Vec<T> {
    #[pure]
    fn len(&self) -> usize;

    #[ensures(self.len() == 0)]
    fn clear(&mut self);
}

/* COUNTEREXAMPLE : 
    Does not even produce a viper file -> no counterexample!
    CE_RM
*/

#[extern_spec]
impl<T> std::vec::Vec<T> {
    #[ensures(self.len() == 0)] //~ ERROR: duplicate specification
    fn clear(&mut self);
}

fn main() {}
