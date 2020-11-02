#![allow(unused_comparisons)]

/// Issue #74: "Pure function call fails with insufficient permissions"

// From: https://github.com/xcaptain/rust-algorithms/blob/master/algorithms/src/search/binary_search.rs

use prusti_contracts::*;

// Prusti Vec wrapper
pub struct VecWrapperusize{
    v: Vec<usize>
}

/* COUNTEREXAMPLE : 
    fn binary_search_iter(arr, target):
        arr <- VecWrapperusize{
            v: [43]
        },
        target <- 42,
        len <- 1,
        old(left) <- 0,
        old(right) <- 0,
        result <- None,
        done <- false,
        condition <- true,
        old(mid) <- 0,
        right <- -1,
        mid <- .....

    TODO_CE : Don't know how or why this fails. An actual bug?


*/

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

// binary search using iteration
#[requires(arr.len() > 0)]
pub fn binary_search_iter(arr: VecWrapperusize, target: usize) -> Option<usize> {
    let len = arr.len();
    let mut left = 0;
    let mut right = len - 1;
    let mut result = None;
    let mut done = false;

    let mut condition = left <= right && !done;
    while condition {
        let mid = (left + right) / 2;
        if arr.lookup(mid) < target { //~ ERROR precondition of pure function call might not hold.
            left = mid + 1;
        } else if arr.lookup(mid) > target {
            right = mid - 1;
        } else {
            result = Some(mid);
            done = false;
        }
        condition = left <= right && !done;
    }

    return result;
}

fn main() {}
