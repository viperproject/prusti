fn rand<T>() -> T { loop {} }
use std::mem::swap;

fn inc_max_three<'a>(mut ma: &'a mut i32, mut mb: &'a mut i32, mut mc: &'a mut i32) {
  if *ma < *mb {
    swap(&mut ma, &mut mb);
  }
  if *mb < *mc {
    swap(&mut mb, &mut mc);
  }
  if *ma < *mb {
    swap(&mut ma, &mut mb);
  }
  *ma += 2;
  *mb += 1;
}
fn inc_max_three_repeat(n: i32, ma: &mut i32, mb: &mut i32, mc: &mut i32) {
  if n != 0 {
    inc_max_three(ma, mb, mc);
    inc_max_three_repeat(n - 1, ma, mb, mc);
  }
}
fn main() {}
fn test() {
  let n = rand();
  let mut a = rand();
  let mut b = rand();
  let mut c = rand();
  inc_max_three_repeat(n, &mut a, &mut b, &mut c);
  assert!(a - b >= n || b - a >= n);
}
