use prusti_contracts::*;


/* COUNTEREXAMPLE:
    fn test3(arg):
        old(arg) <- Number<i32>{
            i: 9000,
            x: 42,
        },
        arg <- Number<i32>{
            i: 8000,
            x: 42
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

#[requires(arg.i >= 9000)]
#[ensures(arg.i >= 8000)]
fn test2<Z>(arg: &mut Number<Z>) {
    // using specs with a *different* typaram
    test1(arg);
}

#[requires(arg.i >= 9000)]
#[ensures(arg.i >= 8001)] //~ ERROR postcondition might not hold
fn test3(arg: &mut Number<i32>) {
    // using specs with a typaram in a non-generic function
    test2(arg);
}

fn main() {}
