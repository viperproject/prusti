fn t1() {
    assert!(0.99_f32.is_nan()); //~ ERROR: the asserted expression might not hold
}

fn t2() {
    let inf = f32::INFINITY;
    assert!(!(inf - inf).is_nan()); //~ ERROR: the asserted expression might not hold
}

fn main() {}