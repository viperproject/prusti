fn fibo(n: i32) -> i32 {
  if n < 1 {
    0
  } else if n == 1 {
    1
  } else {
    fibo(n - 1) + fibo(n - 2)
  }
}

fn main() {}
fn test() {
  let x = 10;
  let result = fibo(x);
  assert!(result == 55);
}
