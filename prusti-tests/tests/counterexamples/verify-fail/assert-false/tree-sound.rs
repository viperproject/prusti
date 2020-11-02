#![feature(nll)]
#![feature(box_patterns)]

use prusti_contracts::*;

struct Tree {
    val: i32,
    left: Option<Box<Tree>>,
    right: Option<Box<Tree>>,
}

/* COUNTEREXAMPLE : 
    fn take_left(tree): 
        tree <- Tree {
            val : 42,
            left : None,
            right : None,
        },
    ret <- None,
    (fails for any other values of the arguments)
*/
fn take_left(tree: Tree) -> Option<Tree> {
    let ret = match tree.left {
        Some(box left) => Some(left),
        None => None,
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn main() {}
