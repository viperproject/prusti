// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/// Converts a tuple of results into a result containing a tuple.


pub fn transpose<U, V, E>(tuple: (Result<U, E>, Result<V, E>)) -> Result<(U, V), E> {
    Ok((tuple.0?, tuple.1?))
}

pub fn range_extract<T: Ord + Copy + Eq + PartialEq + PlusOne>(mut values: Vec<T>) -> Vec<(T, T)> {
    if values.is_empty() {
        return vec![];
    }
    values.sort();
    let mut ranges = vec![];
    let mut curr_range = (values[0], values[0]);
    for value in values.into_iter().skip(1) {
        if value == curr_range.1.plus_one() {
            curr_range.1 = curr_range.1.plus_one()
        } else {
            ranges.push(curr_range);
            curr_range = (value, value);
        }
    }
    ranges.push(curr_range);
    ranges
}

// Increment by one
pub trait PlusOne {
    fn plus_one(self) -> Self;
}

impl PlusOne for i128 {
    fn plus_one(self) -> Self {
        self + 1
    }
}

impl PlusOne for u128 {
    fn plus_one(self) -> Self {
        self + 1
    }
}
