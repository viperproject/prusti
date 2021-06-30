struct Tuple {
    tuple1: i32,
    tuple2: i32,
}

fn main(){}

fn foo(x: Tuple) {
    assert!(x.tuple1 == x.tuple2)
}