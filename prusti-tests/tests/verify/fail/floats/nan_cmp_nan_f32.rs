fn eq32() { 
    assert!(f32::NAN == f32::NAN); //~ ERROR: the asserted expression might not hold
} 
fn ne32() { 
    assert!(f32::NAN != f32::NAN);
}
fn leq32() { 
    assert!(f32::NAN <= f32::NAN); //~ ERROR: the asserted expression might not hold
} 
fn lt32() { 
    assert!(f32::NAN < f32::NAN); //~ ERROR: the asserted expression might not hold
}
fn geq32() { 
    assert!(f32::NAN >= f32::NAN); //~ ERROR: the asserted expression might not hold
} 
fn gt32() { 
    assert!(f32::NAN > f32::NAN); //~ ERROR: the asserted expression might not hold
}

fn main() {}