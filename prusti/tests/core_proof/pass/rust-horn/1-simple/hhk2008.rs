fn rand<T>() -> T { loop {} }

fn main() {}
fn test() {
  let a: i32 = rand();
  let b: i32 = rand();
  let mut res;
  let mut cnt;
  if !(a <= 1000000) {
    return;
  }
  if !(0 <= b && b <= 1000000) {
    return;
  }
  res = a;
  cnt = b;
  while cnt > 0 {
    cnt = cnt - 1;
    res = res + 1;
  }
  assert!(res == a + b);
}
