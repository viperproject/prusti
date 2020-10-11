use prusti_contracts::*;

struct T {
    val: i32
}

//#[after_expiry(x.val == 5)]
//fn identity2(x: &mut T) -> &mut T {
    //x
//}

#[pledge(after_unblocked(x.val) == before_expiry(result.val))]
fn identity(x: &mut T) -> &mut T {
    x
}

fn identity_use() {
    let mut t = T { val: 5 };
    let y = &mut t;
    let z = identity(y);
    z.val = 6;
    assert!(t.val == 6);
}


fn main() {}
