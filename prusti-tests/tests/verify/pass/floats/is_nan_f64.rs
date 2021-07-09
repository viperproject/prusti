fn main() {

    // Can't handle, but can for f32
    //assert!((f64::NAN).is_nan());
    
    assert!(!0.0_f64.is_nan());

    // Can't handle, but can for f32
    // let inf = f64::INFINITY;
    // assert!((inf - inf).is_nan());

}