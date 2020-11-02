use prusti_contracts::*;

pub enum E3 {
    V1(u32),
    V2(u32),
}

/*COUNTEREXAMPLES : (TODO_CE) 
    fn test3(e):
        e <- E3::V1(4),
        b <- ref 4,

        
unregistered verification error, but it still produces
a viper program which produces a counterexample. Probably
because subtraction to unsigned, but is this checked without
overflow checks?
=> is this a pre-verification error? possibly a file to be removed..
*/


pub fn test3(e: &mut E3) -> &mut u32 {  //~ ERROR implicit type invariants might not hold at the end of the method.
    match e {
        E3::V1(ref mut b) => {
            *b -= 5;
            b
        },
        E3::V2(ref mut b) => b,
    }
}

fn main() {}
