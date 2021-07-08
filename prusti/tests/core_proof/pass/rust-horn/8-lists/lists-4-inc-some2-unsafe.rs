// ignore-test: Uses tuples with reference-typed elements

fn rand<T>() -> T { loop {} }

enum List {
  Cons(i32, Box<List>),
  Nil,
}
use List::*;

fn take_some_rest<'a>(mla: &'a mut List) -> (&'a mut i32, &'a mut List) {
  match mla {
    Cons(ma, mla2) => {
      if rand() {
        (ma, mla2)
      } else {
        take_some_rest(mla2)
      }
    }
    Nil => take_some_rest(mla),
  }
}
fn sum(la: &List) -> i32 {
  match la {
    Cons(a, la2) => a + sum(la2),
    Nil => 0,
  }
}
fn main() {}
fn test() {
  let mut la = rand::<List>();
  let n = sum(&la);
  let (mb, mla2) = take_some_rest(&mut la);
  let (mc, _) = take_some_rest(mla2);
  *mb += 1;
  *mc += 1;
  let r = sum(&la);
  assert!(r > n + 2);
}
