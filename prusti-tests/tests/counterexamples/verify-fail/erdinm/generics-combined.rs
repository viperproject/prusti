use prusti_contracts::*;

/* COUNTEREXAMPLE: 
    fn test2<X,Y>(arg):
        old(arg) <- Number<Number<i8, i32, u8>, Number<i16, i32, i64>, Number<isize, i32, usize>>{
            a: Number<Y, i32, u8>{
                a: ?,
                b: 3000,
                c : 43
            },
            b: Number<i16,i32,Y>{
                a: 44,
                b: 5000,
                c: ?
            },
            c: Number<X, i32, usize>{
                a: ?,
                b: 7000,
                c: 47
            }
        },
        arg <- Number<Number<i8, i32, u8>, Number<i16, i32, i64>, Number<isize, i32, usize>>{
            a: Number<Y, i32, u8>{
                a: ?,
                b: 48,
                c : 43
            },
            b: Number<i16,i32,Y>{
                a: 44,
                b: 4000,
                c: ?
            },
            c: Number<X, i32, usize>{
                a: ?,
                b: 6000,
                c: 47
            }
        },
    fn test3<X,Y,Z>(arg):
        old(arg) <- Number<Number<X, i32, Z>, Number<i16, i32, Z>, Number<Y, i32, Y>>{
            a: Number<X, i32, Z>{
                a: ?,
                b: 3000,
                c: ? 
            },
            b: Number<i16,i32,Z>{
                a: 44,
                b: 5000,
                c: ?
            },
            c: Number<Y, i32, Y>{
                a: ?,
                b: 7000,
                c: ?
            }
        },
        arg <- Number<Number<i8, i32, u8>, Number<i16, i32, i64>, Number<isize, i32, usize>>{
            a: Number<Y, i32, Z>{
                a: ?,
                b: 48,
                c: ?
            },
            b: Number<i16,i32,Z>{
                a: 44,
                b: 4000,
                c: ?
            },
            c: Number<Y, i32, Y>{
                a: ?,
                b: 6000,
                c: ?
            }
        },
*/

struct Number<A, B, C> {
    a: A,
    b: B,
    c: C,
}

#[ensures(arg.b == old(arg.b) - 1000)]
fn decr1<D, E>(arg: &mut Number<D, i32, E>) {
    arg.b -= 1000;
}

#[ensures(arg.b.b == old(arg.b.b) - 1000)]
fn decr2<F, G, H, I>(arg: &mut Number<F, Number<G, i32, H>, I>) {
    arg.b.b -= 1000;
}



#[requires(arg.a.b == 3000)]
#[requires(arg.b.b == 5000)]
#[requires(arg.c.b == 7000)]
fn test1<X>(arg: &mut Number<Number<i8, i32, u8>, Number<i16, i32, i64>, Number<X, i32, usize>>) {
    decr1(&mut arg.a);
    decr1(&mut arg.c);
    assert!(arg.a.b == 2000);
    assert!(arg.b.b == 5000);
    assert!(arg.c.b == 6000);
    decr2(arg);
    //assert!(arg.a.b == 2000);
    assert!(arg.b.b == 4000);
    //assert!(arg.c.b == 6000);
}

#[requires(arg.a.b == 3000)]
#[requires(arg.b.b == 5000)]
#[requires(arg.c.b == 7000)]
fn test2<X, Y>(arg: &mut Number<Number<Y, i32, u8>, Number<i16, i32, Y>, Number<X, i32, usize>>) {
    decr1(&mut arg.a);
    decr1(&mut arg.c);
    assert!(arg.a.b == 2000);
    assert!(arg.b.b == 5000);
    assert!(arg.c.b == 6000);
    decr2(arg);
    assert!(arg.a.b == 2000); //~ ERROR the asserted expression might not hold
    assert!(arg.b.b == 4000);
    //assert!(arg.c.b == 6000);
}

#[requires(arg.a.b == 3000)]
#[requires(arg.b.b == 5000)]
#[requires(arg.c.b == 7000)]
fn test3<X, Y, Z>(arg: &mut Number<Number<X, i32, Z>, Number<i16, i32, Z>, Number<Y, i32, Y>>) {
    decr1(&mut arg.a);
    decr1(&mut arg.c);
    assert!(arg.a.b == 2000);
    assert!(arg.b.b == 5000);
    assert!(arg.c.b == 6000);
    decr2(arg);
    //assert!(arg.a.b == 2000);
    assert!(arg.b.b == 4000);
    assert!(arg.c.b == 6000); //~ ERROR the asserted expression might not hold
}

fn main() {}
