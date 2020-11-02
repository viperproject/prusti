use prusti_contracts::*;

/* COUNTEREXAMPLES : 
    fn test3(arg):
        arg <- Number<X> {
            i: 9000,
            x: ?
        }
*/

struct FooBar;

struct Number<X> {
    i: i32,
    x: X,
}

#[requires(arg.i >= 9000)]
#[ensures(arg.i >= 8000)]
fn test1<Y>(arg: &mut Number<Y>) {
    arg.i -= 1000;
}

#[requires(arg.i >= 9001)]
#[ensures(arg.i >= 8000)]
fn test2<Z>(arg: &mut Number<Z>) {
    // using specs with a *different* typaram
    test1(arg);
}

#[requires(arg.i >= 9000)]
#[ensures(arg.i >= 8000)]
fn test3(arg: &mut Number<i32>) {
    // using specs with a typaram in a non-generic function
    test2(arg); //~ ERROR precondition might not hold
}

fn main() {}
