/// Source: https://carllerche.github.io/mio/src/mio/event_imp.rs.html
use prusti_contracts::*;

pub struct Token(pub usize);

pub struct Ready(usize);

pub struct Event {
    kind: Ready,
    token: Token
}

pub struct UnixReady(Ready);

/* 
    COUNTEREXAMPLE : 
        kind_mut(event): 
            event <- Event {
                kind : Ready(32),
                token : Token(42)
            }
            result <- Ready(32),
            
        UnixReady.deref_mut(): 
            self <- UnixReady(Ready(42))
            result <- Ready(42) (or &self.0?)


    or any other instantiation of the arguments
*/

#[ensures(false)] //~ ERROR postcondition
pub fn kind_mut(event: &mut Event) -> &mut Ready {
    &mut event.kind
}

impl UnixReady {
    #[ensures(false)] //~ ERROR postcondition
    fn deref_mut(&mut self) -> &mut Ready {
        &mut self.0
    }
}

fn main() {}
