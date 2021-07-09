fn main() {
    let a:f32 = 1.;
    let b:f32 = 0.;

    assert!(a / b != f32::INFINITY); //~ ERROR: the asserted expression might not hold
}