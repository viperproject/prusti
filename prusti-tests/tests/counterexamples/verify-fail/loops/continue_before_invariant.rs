use prusti_contracts::*;

/* COUNTEREXAMPLE:
    very confusing, this is a bad annotation, not any violated conditions,
    don't think there is a sensible / useful counterexample.
    RM_CE (?)
*/

#[trusted]
fn random(i: u32) -> bool {
    unimplemented!()
}

fn continue_before_invariant() {
    let mut i = 0;
    'myloop: while { //~ ERROR the loop invariant cannot be in a conditional branch of the loop
        if random(i + 2) {
            continue 'myloop;
        }
        random(i + 3)
    } {
        body_invariant!(true);
        i += 1;
    }
}

fn break_before_invariant() {
    let mut i = 0;
    'outer: while random(i + 0) {
        'inner: while {
            if random(i + 2) {
                break 'inner; // Ok
            }
            if random(i + 2) {
                break 'outer; // Ok
            }
            random(i + 3)
        } {
            body_invariant!(true);
            i += 1;
        }
    }
}

fn continue_outer_loop_before_invariant() {
    let mut i = 0;
    'outer: while random(i + 0) {
        'inner: while {
            if random(i + 2) {
                continue 'outer; // Ok
            }
            random(i + 3)
        } {
            body_invariant!(true);
            i += 1;
        }
    }
}

fn main() {}
