use prusti_contracts::*;

// generics: using typarams with different names, and using more-generic
// functions with specifications from less-generic functions

struct FooBar;

struct Number<S, T> {
    i: i32,
    s: S,
    t: T,
}

/* 
COUNTEREXAMPLES : this is one where the counterexample 
probably does not actually violate any conditions but
the tool fails to prove it. 
Still it would make sense if the produces counterexample
would be :
    fn test2a<C>(arg) : 
        old(arg) <- Number<C, i16>{
            i: 9000,
            S: ?,
            T: 42
        },
        arg <- Number{
            i: 7999,
            S: ?,
            T: 42
        }
    fn test2b<D>(arg) 
        old(arg) <- Number<i8, D>{
            i: 8000,
            S: 42,
            T: ?
        },
        arg <- Number{
            i: 6999,
            S: 42,
            T: ?
        }
*/


#[requires(arg.i >= 7000)]
#[ensures(arg.i >= old(arg.i) - 1001)]
fn test1<A, B>(arg: &mut Number<A, B>) {
    arg.i -= 1000;
}

#[requires(arg.i >= 9000)]
#[ensures(arg.i >= 8000)] //~ ERROR postcondition might not hold
fn test2a<C>(arg: &mut Number<C, i16>) {
    test1(arg);
}

#[requires(arg.i >= 8000)]
#[ensures(arg.i >= 7000)] //~ ERROR postcondition might not hold
fn test2b<D>(arg: &mut Number<i8, D>) {
    test1(arg);
}

#[requires(arg.i >= 9000)]
#[ensures(arg.i >= 7000)]
fn test3(arg: &mut Number<i8, i16>) {
    test2a(arg);
    test2b(arg);
}

fn main() {}
