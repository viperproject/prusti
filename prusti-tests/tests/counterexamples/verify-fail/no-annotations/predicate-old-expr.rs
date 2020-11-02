#![feature(nll)]
#![feature(box_patterns)]
#![feature(box_syntax)]

use std::borrow::BorrowMut;


/* COUNTEREXAMPLES : 
    fn lookup(head, index):
        head <- List{
            value: 42,
            next: None
        },
        
*/

struct List {
    value: u32,
    next: Option<Box<List>>,
}

fn lookup(head: &List, index: usize) -> u32 {
    if index == 0 {
        head.value
    } else {
        match head.next {
            Some(box ref tail) => lookup(tail, index - 1),
            None => unreachable!() //~ ERROR unreachable!(..) statement might be reachable
        }
    }
}

fn main() {}
