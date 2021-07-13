// ignore-test: https://github.com/viperproject/prusti-dev/issues/575
fn main() {
    assert!(f64::NAN >= f64::NAN); //~ ERROR: the asserted expression might not hold
}