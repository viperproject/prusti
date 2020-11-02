#![feature(box_syntax)]

use prusti_contracts::*;

#[trusted]
fn random() -> bool {
    unimplemented!()
}

/* COUNTEREXAMPLE : 
    (this will always fail)
    fn test():
        old(y) <- None,
        z <- None,
        x <- box 5
        t0 (result of random() at L31) <- true,     
                            (note : this is new, but it might be very helpful
                            to display values that are never stored in a variabel
                            but still used)
        y <- Some(x)

        

*/

fn test() {
    let mut y = None;
    let mut z = None;

    loop {
        let x = box 5;
        if random() {
            y = Some(x);
        } else {
            z = Some(x);
        }
        assert!(false); //~ ERROR might not hold
    }
}

fn main() {}
