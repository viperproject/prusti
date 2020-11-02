#![allow(unused_comparisons)]

/// Issue #73: "Properly report failing precondition of pure functions"

// From: https://github.com/xcaptain/rust-algorithms/blob/master/algorithms/src/search/binary_search.rs

/* COUNTEREXAMPLE:
    fn binary_search_rec(arr, target):
        arr <- VecWrapperusize{
            v: []
        },
        target <- 42,
        len <- 0,
    fn binary_search_help(arr, left, right, target):
        arr <- VecWrapperusize{
            v: []
        },
        left <- 0,
        right <- 4,
        target <- 42,
        mid <- 2,

*/

use prusti_contracts::*;

// Prusti Vec wrapper
pub struct VecWrapperusize{
    v: Vec<usize>
}

impl VecWrapperusize {
    // Encoded as body-less Viper function
    #[trusted]
    #[pure]
    pub fn len(&self) -> usize {
        self.v.len()
    }

    // Encoded as body-less Viper method
    #[trusted]
    #[ensures(result.len() == length)]
    #[ensures(forall(|i: usize| (0 <= i && i < length) ==> result.lookup(i) == 0))]
    pub fn new(length: usize) -> Self {
        VecWrapperusize{ v: vec![0; length] }
    }

    // Encoded as body-less Viper function
    #[trusted]
    #[pure]
    #[requires(0 <= index && index < self.len())]
    pub fn lookup(&self, index: usize) -> usize {
        self.v[index]
    }

    // Encoded as body-less Viper method
    #[trusted]
    #[requires(0 <= index && index < self.len())]
    #[ensures(self.lookup(old(index)) == old(value))]
    pub fn store(&mut self, index: usize, value: usize) {
        self.v[index] = value;
    }
}

// binary search using recursion
pub fn binary_search_rec(arr: VecWrapperusize, target: usize) -> Option<usize> {
    let len = arr.len();
    return binary_search_help(arr, 0, len - 1, target); //~ ERROR type invariant expected by the function call might not hold.
}

// Here the precondition is missing
fn binary_search_help(arr: VecWrapperusize, left: usize, right: usize, target: usize) -> Option<usize> {
    if left <= right {
        let mid = (left + right) / 2;
        if arr.lookup(mid) < target { //~ ERROR precondition of pure function call might not hold
            return binary_search_help(arr, mid + 1, right, target);
        } else if arr.lookup(mid) > target {
            return binary_search_help(arr, left, mid - 1, target);
        } else {
            return Some(mid);
        }
    }
    return None;
}

fn main() {}
