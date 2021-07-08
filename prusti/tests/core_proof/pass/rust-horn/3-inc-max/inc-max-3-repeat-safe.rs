fn rand<T>() -> T { loop {} }

fn take_max<'a>(ma: &'a mut i32, mb: &'a mut i32) -> &'a mut i32 {
  if *ma >= *mb {
    ma
  } else {
    mb
  }
}
fn inc_max_repeat(n: i32, ma: &mut i32, mb: &mut i32) {
  if n != 0 {
    let mc = take_max(ma, mb);
    *mc += 1;
    inc_max_repeat(n - 1, ma, mb);
  }
}
fn main() {}
fn test() {
  let n = rand();
  let mut a = rand();
  let mut b = rand();
  inc_max_repeat(n, &mut a, &mut b);
  assert!(a - b >= n || b - a >= n);
}
