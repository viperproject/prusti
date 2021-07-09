fn foo(x: f64) {

    if x < 0. {}
    else if  x >= 0. {}
    else {
        unreachable!() //~ ERROR: statement might be reachable
    }
}

fn main() {}