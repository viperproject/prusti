// ignore-test: https://github.com/viperproject/prusti-dev/issues/575
fn eq64() { 
    assert!(f64::NAN == f64::NAN); //~ ERROR: the asserted expression might not hold
}
fn ne64() { 
    assert!(f64::NAN != f64::NAN); 
} 
fn leq64() { 
    assert!(f64::NAN <= f64::NAN); //~ ERROR: the asserted expression might not hold
} 
fn lt64() { 
    assert!(f64::NAN < f64::NAN); //~ ERROR: the asserted expression might not hold
}
fn geq64() { 
    assert!(f64::NAN >= f64::NAN); //~ ERROR: the asserted expression might not hold
} 
fn gt64() { 
    assert!(f64::NAN > f64::NAN); //~ ERROR: the asserted expression might not hold
}

fn main() {}