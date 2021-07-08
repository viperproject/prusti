fn rand<T>() -> T { loop {} }

enum List {
  Cons(i32, Box<List>),
  Nil,
}
use List::*;

fn append<'a>(mla: &'a mut List, lb: List) {
  match mla {
    Cons(_, mla2) => append(mla2, lb),
    Nil => *mla = lb,
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
  let lb = rand::<List>();
  let m = sum(&la);
  let n = sum(&lb);
  append(&mut la, lb);
  let r = sum(&la);
  assert!(r > m + n);
}
