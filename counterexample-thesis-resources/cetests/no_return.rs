use prusti_contracts::*;

fn foo(x:i32, y:i32) {
    let z = if x == 42 {
        35
    } else {
        x + 4
    };
    assert!(z != y + 5);
}

fn main() {}