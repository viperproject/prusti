use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn S.test(self):
        empty

*/

struct S {
    f: i32
}

impl S {
    #[requires(true)]
    #[ensures(false)] //~ ERROR postcondition
    pub fn test(self) {}
}

fn main() {}
