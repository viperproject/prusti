use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn main():
        empty
*/

#[ensures(false)] //~ ERROR postcondition
fn main() {
}
