use prusti_contracts::*;

/* COUNTEREXAMPLES : 
    fn test2(pair):
        pair <- Pair<i8, U> {
                a: 1,
                b: ? 
                }
    (possibly not supported because of generic types)
    (fails if pair.a != 42)
*/

struct Pair<T, U> {
    a: T,
    b: U,
}

//#[requires(pair.a == 42)]
fn test2<U>(pair: &mut Pair<i8, U>) {
    assert!(pair.a == 42); //~ ERROR the asserted expression might not hold
}

fn main() {}
