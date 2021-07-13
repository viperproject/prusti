fn main() {
    let a = 0.4 + 0.2;
    assert!(a == 0.6); //~ ERROR: the asserted expression might not hold
}