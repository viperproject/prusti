#![feature(nll)]
#![feature(box_patterns)]
#![feature(box_syntax)]

use prusti_contracts::*;

use std::borrow::BorrowMut;

/* COUNTEREXAMPLE : 
    fn lookup(head, index):
        head <- List{
            value : 42,
            next : None
        },
        index <- 1,

    fn lookup2(head, index):
        head <- List{
            value : 42,
            next : None
        },
        index <- 1

    fn lookup3(head, index):
        head <- List{
            value : 42,
            next : None
        },
        index <- 1

    fn lookup4(head, index):
        head <- List{
            value : 42,
            next : None
        },
        index <- 1

    fn lookup5(head, index):
        head <- List{
            value : 42,
            next : None
        },
        index <- 1
*/

struct List {
    value: u32,
    next: Option<Box<List>>,
}

#[allow(unconditional_recursion)]
fn diverging() -> ! {
    diverging()
}

#[pure]
fn lookup(head: &List, index: isize) -> u32 {
    if index == 0 {
        head.value
    } else {
        match head.next {
            Some(box ref tail) => lookup(tail, index - 1),
            None => diverging() //~ ERROR diverging
        }
    }
}

#[pure]
fn lookup2(head: &List, index: isize) -> u32 {
    if index == 0 {
        head.value
    } else {
        match head.next {
            Some(box ref tail) => lookup(tail, index - 1),
            None => panic!() //~ ERROR might panic
        }
    }
}

#[pure]
fn lookup3(head: &List, index: isize) -> u32 {
    if index == 0 {
        head.value
    } else {
        match head.next {
            Some(box ref tail) => lookup(tail, index - 1),
            None => unreachable!() //~ ERROR might be reachable
        }
    }
}

#[pure]
fn lookup4(head: &List, index: isize) -> u32 {
    if index == 0 {
        head.value
    } else {
        match head.next {
            Some(box ref tail) => lookup(tail, index - 1),
            None => unimplemented!() //~ ERROR might be reachable
        }
    }
}

#[pure]
fn lookup5(head: &List, index: isize) -> u32 {
    if index == 0 {
        head.value
    } else {
        match head.next {
            Some(box ref tail) => lookup(tail, index - 1),
            None => {
                assert!(false); //~ ERROR might not hold
                0
            }
        }
    }
}

fn main() {}
