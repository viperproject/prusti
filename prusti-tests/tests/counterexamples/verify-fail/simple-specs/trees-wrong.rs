#![allow(unused_comparisons)]

/// Problem 3 (Trees) from Program Verification 2018 Project 3.
///
/// TODO: Casts: i as usize

/* COUNTEREXAMPLE: 
    fn do_solve(i, fruits):
        i <- 5,
        fruits <- VecWrapperU32{
            v: [1,2,3]
        },
        
*/


use prusti_contracts::*;

pub struct VecWrapperU32{
    v: Vec<u32>
}

impl VecWrapperU32 {
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
        VecWrapperU32{ v: vec![0; length] }
    }

    // Encoded as body-less Viper function
    #[trusted]
    #[pure]
    #[requires(0 <= index && index < self.len())]
    pub fn lookup(&self, index: usize) -> u32 {
        self.v[index]
    }

    // Encoded as body-less Viper method
    #[trusted]
    #[requires(0 <= index && index < self.len())]
    #[ensures(self.lookup(old(index)) == old(value))]
    pub fn store(&mut self, index: usize, value: u32) {
        self.v[index] = value;
    }
}


#[trusted]
fn to_usize(number: isize) -> usize {
    number as usize
}

fn max(a: u32, b: u32) -> u32 {
    if a > b {
        a
    } else {
        b
    }
}

fn do_solve1(i: isize, fruits: &mut VecWrapperU32) -> u32 {
    if i <= -1 {
        0
    } else {
        //do_solve1(i - 1, fruits)
        max(
            fruits.lookup(to_usize(i)) //~ ERROR precondition
                + do_solve1(i - 2, fruits),
            do_solve1(i - 1, fruits)
        )
    }
}

fn main() {}
