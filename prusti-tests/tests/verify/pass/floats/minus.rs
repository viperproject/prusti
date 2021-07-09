fn main() {
    let a  = 10. - 10.0000001;
    assert!(a < 0.);
}