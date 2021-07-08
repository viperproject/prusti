fn rand<T>() -> T { loop {} }

fn assume(cond: bool) {
  if !cond {
    loop {}
  }
}
fn main() {}
fn test() {
  let mut x: i32;
  let mut y: i32;
  let mut z: i32;
  let p1;
  let p2;
  let p3;
  let a: i32 = rand();
  assume(a > 0);
  x = rand();
  y = rand();
  z = rand();
  if rand() {
    p1 = 1;
  } else {
    p1 = 0;
  }
  while rand() {}
  if rand() {
    p2 = 1;
  } else {
    p2 = 0;
  }
  while rand() {}
  if rand() {
    p3 = 1;
  } else {
    p3 = 0;
  }
  while rand() {}
  if p1 != 0 {
    x += 1;
  } else {
    x -= 1;
  }
  while rand() {}
  if p2 != 0 {
    y += 1;
  } else {
    y -= 1;
  }
  while rand() {}
  if p3 != 0 {
    z += 1;
  } else {
    z -= 1;
  }
  assert!(a > 0);
}
