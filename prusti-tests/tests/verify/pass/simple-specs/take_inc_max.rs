use prusti_contracts::*;

/// Example 1 / Figure 1 from the paper "RustHorn: CHC-based Verification for Rust Programs"
/// authored by Yusuke Matsushita, Takeshi Tsukada, and Naoki Kobayashi

#[ensures(*result >= old(*ma) && *result >= old(*mb))]
#[pledge(if old(*ma >= *mb) {
        (after_unblocked(*ma) == before_expiry(*result) && after_unblocked(*mb) == old(*mb))
    } else {
        (after_unblocked(*ma) == old(*ma) && after_unblocked(*mb) == before_expiry(*result))
    })]
fn take_max<'a>(ma: &'a mut i32, mb: &'a mut i32) -> &'a mut i32 {
    if *ma >= *mb { ma } else { mb }
}

#[ensures(result == true)]
fn inc_max(mut a: i32, mut b: i32) -> bool {
    // Optional, to avoid the warning on the mutable arguments
    // let mut a = a;
    // let mut b = b;
    {
        let mc = take_max(&mut a, &mut b);
        *mc += 1;
    }
    a != b
}

fn main() {}
