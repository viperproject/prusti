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
fn main() {}
fn test() {
  let mut a = rand();
  let mut b = rand();
  let mut c = rand();
  inc_max_three(&mut a, &mut b, &mut c);
  assert!(a != b + 2);
}
