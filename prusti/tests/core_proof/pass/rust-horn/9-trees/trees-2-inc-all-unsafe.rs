fn rand<T>() -> T { loop {} }

enum Tree {
  Node(Box<Tree>, i32, Box<Tree>),
  Leaf,
}
use Tree::*;

fn inc<'a>(mta: &'a mut Tree) {
  match mta {
    Node(mtal, mx, mtar) => {
      inc(mtal);
      *mx += 1;
      inc(mtar);
    }
    Leaf => {}
  }
}
fn sum(ta: &Tree) -> i32 {
  match ta {
    Node(tal, a, tar) => sum(tal) + a + sum(tar),
    Leaf => 0,
  }
}
fn size(ta: &Tree) -> i32 {
  match ta {
    Node(tal, _, tar) => size(tal) + 1 + size(tar),
    Leaf => 0,
  }
}
fn main() {}
fn test() {
  let mut ta = rand::<Tree>();
  let n = sum(&ta);
  let s = size(&ta);
  inc(&mut ta);
  let r = sum(&ta);
  assert!(r > n + s);
}
