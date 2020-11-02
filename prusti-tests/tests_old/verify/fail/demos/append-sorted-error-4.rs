#![feature(box_patterns, box_syntax)]
use prusti_contracts::*;

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

/* COUNTEREXAMPLE : not supported because of boxes and infinite
    enums */
#[requires(a.sorted())]
#[ensures(a.len() == old(a.len()) + 1)]
#[ensures(a.get(0) == old(a.get(0)))]
#[ensures(a.sorted())] //~ ERROR postcondition might not hold
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

#[requires(a.sorted() && a.get(a.len() - 1) <= 100)]
#[ensures(a.sorted())]
fn client(a: &mut List, b: &mut List) {
    let old_len = b.len();
    append(a, 100);
    assert!(b.len() == old_len);
}

fn main() {}
