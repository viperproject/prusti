use prusti_contracts::*;

/* COUNTEREXAMPLE : 
    fn Foo::baz(self, c): 
        self <- Dummy,
        c <- 0,
        res <- -1

    fn test_foo_pre(): 
        d <- Dummy,


    fn test_bar_2_pre(): 
        d <- Dummy

    fn test_bar_2_post():
        d <- Dummy,
        res <- 100
*/



trait Foo {
    #[requires(-10 <= a && a <= 10)]
    #[ensures(result > 0)]
    fn foo(&self, a: isize) -> isize;

    #[requires(-5 <= b && b <= 5)]
    #[ensures(result > 10)]
    fn bar(&self, b: isize) -> isize {
        assert!(-5 <= b && b <= 5); // Ok
        100
    }

    #[requires(-1 <= c && c <= 1)]
    #[ensures(result > 1)] //~ ERROR postcondition
    fn baz(&self, c: isize) -> isize {
        assert!(-1 <= c && c <= 1); // Ok
        100
    }
}

struct Dummy;

impl Foo for Dummy {
    fn foo(&self, a: isize) -> isize {
        assert!(-10 <= a && a <= 10); // Ok
        10
    }

    fn baz(&self, c: isize) -> isize {
        assert!(-1 <= c && c <= 1); // Ok
        -1
    }
}

fn test_foo_pre() {
    let d = Dummy;
    d.foo(100); //~ ERROR precondition
}

fn test_foo_post() {
    let d = Dummy;
    let res = d.foo(10);
    assert!(res > 0); // Ok
}

fn test_bar_1_pre() {
    let d = Dummy;
    d.bar(5); // Ok
}

fn test_bar_2_pre() {
    let d = Dummy;
    d.bar(100); //~ ERROR precondition
}

fn test_bar_1_post() {
    let d = Dummy;
    let res = d.bar(5);
    assert!(res > 10); // Ok
}

fn test_bar_2_post() {
    let d = Dummy;
    let res = d.bar(5);
    assert!(res > 100); //~ ERROR
}

fn main(){}
