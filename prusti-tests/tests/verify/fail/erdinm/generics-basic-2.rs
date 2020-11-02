use prusti_contracts::*;

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

/* TODO : what does trusted mean? why does assert!(valid(u)) fail if
          it always returns true.
   COUNTEREXAMPLE : 
*/
#[requires(valid(u))]
fn test<U>(u: &mut U) {
    assert!(valid(u));
    read(u);
    assert!(valid(u));
    write(u);
    assert!(valid(u)); //~ ERROR the asserted expression might not hold
}

fn main() {}
