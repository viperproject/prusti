fn rand<T>() -> T { loop {} }

enum Tree {
  Node(Box<Tree>, i32, Box<Tree>),
  Leaf,
}
use Tree::*;

fn append<'a>(mta: &'a mut Tree, tb: Tree) {
  match mta {
    Node(mtal, _, mtar) => {
      if rand() {
        append(mtal, tb)
      } else {
        append(mtar, tb)
      }
    }
    Leaf => {
      *mta = tb;
    }
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
  let tb = rand::<Tree>();
  let m = sum(&ta);
  let n = sum(&tb);
  append(&mut ta, tb);
  let r = sum(&ta);
  assert!(r == m + n);
}
