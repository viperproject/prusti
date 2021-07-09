fn main() {
    let a = 5.0000000000000001;
    let b = 5.;

    assert!(a - b  > 0.); //~ ERROR: the asserted expression might not hold
}