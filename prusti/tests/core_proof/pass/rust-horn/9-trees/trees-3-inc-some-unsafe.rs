fn rand<T>() -> T { loop {} }

enum Tree {
  Node(Box<Tree>, i32, Box<Tree>),
  Leaf,
}
use Tree::*;

fn take_some<'a>(mta: &'a mut Tree) -> &'a mut i32 {
  match mta {
    Node(mtal, ma, mtar) => {
      if rand() {
        ma
      } else if rand() {
        take_some(mtal)
      } else {
        take_some(mtar)
      }
    }
    Leaf => take_some(mta),
  }
}
fn sum(ta: &Tree) -> i32 {
  match ta {
    Node(tal, x, tar) => sum(tal) + x + sum(tar),
    Leaf => 0,
  }
}
fn main() {}
fn test() {
  let mut ta = rand::<Tree>();
  let n = sum(&ta);
  let mb = take_some(&mut ta);
  *mb += 1;
  let r = sum(&ta);
  assert!(r > n + 1);
}
