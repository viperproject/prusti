use prusti_contracts::*;

/* COUNTEREXAMPLE : (CE_RM)
    This actually does verify!
    Does this belong here?
    
    fn test1(a, value):

    fn test2(a):

    fn test4(a):
*/

struct U {
    f: u32,
}

fn test1(a: &mut U, value: u32) {   //~ ERROR implicit type invariants might not hold at the end of the method.
    a.f -= value;
}

fn test2(a: &mut U) -> &mut u32 { //~ ERROR implicit type invariants might not hold at the end of the method.
    a.f -= 5;
    &mut a.f
}

fn test3(a: u32) {
    let b = a - 5;
}

fn test4(a: u32) {
    let b = a - 5;
    test3(b); //~ ERROR implicit type invariant expected by the function call might not hold.
}


fn main() {}
