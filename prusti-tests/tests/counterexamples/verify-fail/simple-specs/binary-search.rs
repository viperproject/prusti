// From: https://github.com/xcaptain/rust-algorithms/blob/master/algorithms/src/search/binary_search.rs

/* COUNTEREXAMPLES :
    fn binary_search_rec(arr, target):
        arr <- VecWrapperusize{
            v: []
        },
        target <- 42,
        len <- 0,
    (problem because of subtracting one from 0 : usize)

    fn binary_search_help(arr, left, right, target):
        arr <- VecWrapperusize{
            v: [1,2,3]
        },
        left <- 4,
        right <- 6,
        mid <- 5,

    fn binary_search_iter(arr, target):
        ???
        (TODO_CE)

*/


#![allow(unused_comparisons)]
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
    binary_search_help(arr, 0, len - 1, target) //~ ERROR type invariant expected by the function call might not hold.
}

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
        if arr.lookup(mid) < target { //~ ERROR precondition of pure function call might not hold
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

/*
assert_eq! not currently supported

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_binary_search_rec_yes() {
        assert_eq!(Some(0), binary_search_rec(vec![1, 2, 3], 1));
    }

    #[test]
    fn test_binary_search_rec_no() {
        assert_eq!(None, binary_search_rec(vec![1, 2, 3], 10));
    }

    #[test]
    fn test_binary_search_rec_last() {
        assert_eq!(Some(2), binary_search_rec(vec![1, 2, 3], 3));
    }

    // for iter version
    #[test]
    fn test_binary_search_iter_yes() {
        assert_eq!(Some(0), binary_search_iter(vec![1, 2, 3], 1));
    }

    #[test]
    fn test_binary_search_iter_no() {
        assert_eq!(None, binary_search_iter(vec![1, 2, 3], 10));
    }

    #[test]
    fn test_binary_search_iter_last() {
        assert_eq!(Some(2), binary_search_iter(vec![1, 2, 3], 3));
    }
}
*/

fn main() {}
