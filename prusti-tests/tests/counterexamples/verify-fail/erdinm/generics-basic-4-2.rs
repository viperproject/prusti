use prusti_contracts::*;

// generics: using typarams with different names, and using more-generic
// functions with specifications from less-generic functions

/* COUNTEREXAMPLE: (TODO_CE)
bad annotations cause this, the property will not actually
be violated. hard to tell what counterexample will look like.
could be something like:    
    
    fn test3(arg):
        old(arg) <- Number<i8,i16>{
            i:9000,
            s:42,
            t:43
        },
        arg <- Number<i8,i16>{
            i:7000,
            s:42,
            t:43
        }

*/

struct FooBar;

struct Number<S, T> {
    i: i32,
    s: S,
    t: T,
}

#[requires(arg.i >= 7000)]
#[ensures(arg.i >= old(arg.i) - 1000)]
fn test1<A, B>(arg: &mut Number<A, B>) {
    arg.i -= 1000;
}

#[requires(arg.i >= 9000)]
#[ensures(arg.i >= 7000)]
fn test2a<C>(arg: &mut Number<C, i16>) {
    test1(arg);
}

#[requires(arg.i >= 8000)]
#[ensures(arg.i >= 7000)]
fn test2b<D>(arg: &mut Number<i8, D>) {
    test1(arg);
}

#[requires(arg.i >= 9000)]
#[ensures(arg.i >= 7000)]
fn test3(arg: &mut Number<i8, i16>) {
    test2a(arg);
    test2b(arg); //~ ERROR precondition might not hold
}

fn main() {}
