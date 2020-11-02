#![feature(box_patterns, box_syntax)]
use prusti_contracts::*;

/* COUNTEREXAMPLES : 
    fn client(a, b): 
        a <- List {
            val : 111,
            next : None,
        },
        b <- List {
            val : 42,
            next : None,
        },
        old_len <- 1,

        (violates precondition of append if a.val > 100
        and is only valid input if a.val <= 111)
*/


struct List {
    val: i32,
    next: Option<Box<List>>
}

impl List {
    #[pure]
    #[ensures(result >= 0)]
    fn len(&self) -> usize {
        match self.next {
            None => 1,
            Some(box ref tail) => tail.len() + 1
        }
    }

    #[pure]
    #[requires(0 <= index && index < self.len())]
    fn get(&self, index: usize) -> i32 {
        if index == 0 {
            self.val
        } else {
            if let Some(box ref tail) = self.next {
                tail.get(index - 1)
            } else {
                unreachable!()
            }
        }
    }

    #[pure]
    fn sorted(&self) -> bool {
        if let Some(box ref tail) = self.next {
            self.val <= tail.val && tail.sorted()
        } else {
            true
        }
    }
}

#[requires(a.sorted() && a.get(a.len() - 1) <= v)]
#[ensures(a.len() == old(a.len()) + 1)]
#[ensures(a.get(0) == old(a.get(0)))]
#[ensures(a.sorted())]
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


#[requires(a.sorted() && a.get(a.len() - 1) <= 111)]
#[ensures(a.sorted())]
fn client(a: &mut List, b: &mut List) {
    let old_len = b.len();
    append(a, 100); //~ ERROR precondition might not hold
    assert!(b.len() == old_len);
}

fn main() {}
