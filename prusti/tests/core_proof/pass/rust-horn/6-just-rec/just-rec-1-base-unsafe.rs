fn rand<T>() -> T { loop {} }

fn just_rec(ma: &mut i32) {
  if rand() {
    return;
  }
  let mut b = rand();
  just_rec(&mut b);
}
fn main() {}
fn test() {
  let mut a = rand();
  let a0 = a;
  just_rec(&mut a);
  assert!(a0 > a);
}
