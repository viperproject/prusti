fn rand<T>() -> T { loop {} }

fn main() {}
fn test() {
  let mut x = 1;
  let mut y = 1;
  if rand() {
    x += 1;
    y += 1;
  }
  while rand() {}
  if rand() {
    x += 1;
    y += 1;
  }
  while rand() {}
  if rand() {
    x += 1;
    y += 1;
  }
  while rand() {}
  if rand() {
    x += 1;
    y += 1;
  }
  while rand() {}
  if rand() {
    x += 1;
    y += 1;
  }
  while rand() {}
  if rand() {
    x += 1;
    y += 1;
  }
  while rand() {}
  if rand() {
    x += 1;
    y += 1;
  }
  assert!(x > y);
}
