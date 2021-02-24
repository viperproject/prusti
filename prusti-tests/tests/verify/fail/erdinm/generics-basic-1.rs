use prusti_contracts::*;

struct Pair<T, U> {
    a: T,
    b: U,
}

//#[requires(pair.a == 42)]
fn test2<U>(pair: &mut Pair<i8, U>) {
    assert!(pair.a == 42); //~ ERROR the asserted expression might not hold
}

fn main() {}
