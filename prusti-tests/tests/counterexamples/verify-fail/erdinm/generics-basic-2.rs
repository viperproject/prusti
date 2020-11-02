use prusti_contracts::*;

/* COUNTEREXAMPLES : 
    fn test<U>(u):
        old(u) <- ?
        u <- ?
    
    (this is probably the best we could hope for, 
    because from this we can derive that u has changed)
*/

#[pure]
#[trusted] // pretend to be abstract
fn valid<U>(u: &U) -> bool {
    true
}

#[pure]
fn read<U>(u: &U) -> i32 {
    42
}

fn write<U>(u: &mut U) {
}

#[requires(valid(u))]
fn test<U>(u: &mut U) {
    assert!(valid(u));
    read(u);
    assert!(valid(u));
    write(u);
    assert!(valid(u)); //~ ERROR the asserted expression might not hold
}

fn main() {}
