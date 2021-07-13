fn main() {
    let a:f32 = 1.;
    let b:f32 = 0.;
    let z = a / b;
    assert!(z == f32::INFINITY);
}