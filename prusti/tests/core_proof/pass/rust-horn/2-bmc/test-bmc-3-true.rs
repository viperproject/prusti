fn rand<T>() -> T { loop {} }

fn main() {}
fn test() {
  let mut p = 0;
  if rand() {
    p += 1;
  } else {
    p += 2;
  }
  if rand() {
    p += 1;
  } else {
    p += 2;
  }
  if rand() {
    p += 1;
  } else {
    p += 2;
  }
  if rand() {
    p += 1;
  } else {
    p += 2;
  }
  if rand() {
    p += 1;
  } else {
    p += 2;
  }
  if rand() {
    p += 1;
  } else {
    p += 2;
  }
  assert!(p <= 12);
}
