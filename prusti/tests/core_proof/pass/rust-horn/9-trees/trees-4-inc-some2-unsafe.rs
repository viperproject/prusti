// ignore-test: Uses tuples with reference-typed elements

fn rand<T>() -> T { loop {} }

enum Tree {
  Node(Box<Tree>, i32, Box<Tree>),
  Leaf,
}
use Tree::*;

fn take_some_rest<'a>(mta: &'a mut Tree) -> (&'a mut i32, &'a mut Tree) {
  match mta {
    Node(mtal, ma, mtar) => {
      if rand() {
        (ma, if rand() { mtal } else { mtar })
      } else if rand() {
        take_some_rest(mtal)
      } else {
        take_some_rest(mtar)
      }
    }
    Leaf => take_some_rest(mta),
  }
}
fn sum(ta: &Tree) -> i32 {
  match ta {
    Node(tal, a, tar) => sum(tal) + a + sum(tar),
    Leaf => 0,
  }
}
fn main() {}
fn test() {
  let mut ta = rand::<Tree>();
  let n = sum(&ta);
  let (mb, mta2) = take_some_rest(&mut ta);
  let (mc, _) = take_some_rest(mta2);
  *mb += 1;
  *mc += 1;
  let r = sum(&ta);
  assert!(r > n + 2);
}
