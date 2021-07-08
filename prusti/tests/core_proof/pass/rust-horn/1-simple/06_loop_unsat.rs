fn rand<T>() -> T { loop {} }

fn main() {}
fn test() {
  let mut x = 1;
  let mut y = 0;
  while rand() {
    x = x + y;
    y += 1;
  }
  assert!(x >= y);
}
