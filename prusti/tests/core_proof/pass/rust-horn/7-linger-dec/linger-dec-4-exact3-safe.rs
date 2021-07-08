fn rand<T>() -> T { loop {} }

fn linger_dec_bound_three(n: i32, ma: &mut i32, mb: &mut i32, mc: &mut i32) {
  if n == 0 {
    return;
  }
  *ma -= 1;
  *mb -= 2;
  *mc -= 3;
  let mut d = rand();
  let mut ma = ma;
  let mut mb = mb;
  let mut mc = mc;
  if rand() {
    ma = &mut d;
  } else if rand() {
    mb = &mut d;
  } else if rand() {
    mc = &mut d;
  }
  linger_dec_bound_three(n - 1, ma, mb, mc);
}
fn main() {}
fn test() {
  let n = rand();
  let mut a = rand();
  let mut b = rand();
  let mut c = rand();
  let a0 = a;
  linger_dec_bound_three(n, &mut a, &mut b, &mut c);
  assert!(a0 >= a && a0 - a <= 3 * n);
}
