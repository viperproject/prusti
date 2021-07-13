fn main() {    
    let x:f64 = 0.99;
    assert!(x.is_nan()); //~ ERROR: the asserted expression might not hold
}
