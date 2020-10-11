use prusti_contracts::*;

struct Point(u32, u32);

#[pledge(before_expiry(*result) == after_unblocked(*v))]
//~^ ERROR pledge in the postcondition might not hold.
#[pledge(before_expiry(*result) == after_unblocked(*v-1))]
fn f0(v: &mut u32) -> &mut u32 {
    v
}

fn main() {
}
