fn rand<T>() -> T { loop {} }

fn take_max<'a>(ma: &'a mut i32, mb: &'a mut i32) -> &'a mut i32 {
  if *ma >= *mb {
    ma
  } else {
    mb
  }
}
fn main() {}
fn test() {
  let mut a = rand();
  let mut b = rand();
  let mc = take_max(&mut a, &mut b);
  *mc += 1;
  assert!(a != b + 1);
}
