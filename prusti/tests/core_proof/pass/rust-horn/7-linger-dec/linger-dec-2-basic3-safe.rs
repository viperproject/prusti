fn rand<T>() -> T { loop {} }

fn linger_dec_three(ma: &mut i32, mb: &mut i32, mc: &mut i32) {
  *ma -= 1;
  *mb -= 2;
  *mc -= 3;
  if rand() {
    return;
  }
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
  linger_dec_three(ma, mb, mc);
}
fn main() {}
fn test() {
  let mut a = rand();
  let mut b = rand();
  let mut c = rand();
  let a0 = a;
  linger_dec_three(&mut a, &mut b, &mut c);
  assert!(a0 > a);
}
