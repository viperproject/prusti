extern crate prusti_contracts;
use prusti_contracts::*;


/* COUNTEREXAMPLE:
    Does not produce viper file, fails pre verification
    CE_RM
*/


#[extern_spec]
impl<T> std::vec::Vec<T> {
    /// Wrong method name
    #[ensures(self.len() == 0)] //~ ERROR no function or associated item
    fn clears(&mut self);
}

fn main() {}
