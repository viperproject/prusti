/// Source: https://doc.servo.org/src/fixedbitset/lib.rs.html#18-21

/* COUNTEREXAMPLE :
fn div_rem(x,d):
    x <- 42,
    d <- 0,

*/


#[inline]
fn div_rem(x: usize, d: usize) -> (usize, usize)
{
    (
        x / d,  //~ ERROR assertion might fail with "attempt to divide by zero"
        x % d
    )
}

fn main(){}
