#![feature(box_patterns, box_syntax)]
use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn client(a, b): 
        old(a) <- List{
            val: 42,
            next: None
        },
        b <- List {
            val : 43,
            next : None
        },
        old_len <- 1,
        a <- List {
            val : 42,
            next : Some(box List {
                        val : 100,
                        next : None,
            }
        } 

    (any other instantiation of a and b would also be a valid counterexample)
    (probably not supported because of recursive enum and boxes)


*/

struct List {
    val: i32,
    next: Option<Box<List>>
}

impl List {
    #[pure]
    fn len(&self) -> usize {
        match self.next {
            None => 1,
            Some(box ref tail) => tail.len() + 1
        }
    }
}

fn append(a: &mut List, v: i32) {
    if let Some(box ref mut tail) = a.next {
        append(tail, v);
    } else {
        a.next = Some(box List {
            val: v,
            next: None
        });
    }
}

// recursive struct
fn client(a: &mut List, b: &mut List) {
    let old_len = b.len();
    append(a, 100);
    assert!(b.len() != old_len); //~ ERROR
}

fn main() {}
