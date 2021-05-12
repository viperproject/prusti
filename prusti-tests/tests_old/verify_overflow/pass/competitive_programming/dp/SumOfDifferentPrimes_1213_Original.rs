// compile-flags: -Pdisable_more_complete_exhale
// https://onlinejudge.org/external/12/1213.pdf
use prusti_contracts::*;

pub struct VecWrapperI32 {
    v: Vec<isize>,
}

impl VecWrapperI32 {
    #[trusted]
    #[pure]
    #[ensures (0 <= result)]
    pub fn len(&self) -> isize {
        self.v.len() as isize
    }

    #[trusted]
    #[ensures (result.len() == 0)]
    pub fn new() -> Self {
        VecWrapperI32 { v: Vec::new() }
    }

    #[trusted]
    #[pure]
    #[requires (0 <= index && index < self.len())]
    pub fn lookup(&self, index: isize) -> isize {
        self.v[index as usize]
    }

    #[trusted]
    #[ensures (self.len() == old(self.len()) + 1)]
    #[ensures (self.lookup(old(self.len())) == value)]
    #[ensures (forall(|i: isize| (0 <= i && i < old(self.len())) ==>
                    self.lookup(i) == old(self.lookup(i))))]
    pub fn push(&mut self, value: isize) {
        self.v.push(value);
    }
}

struct Matrix {
    _ghost_z_size: usize,
    _ghost_y_size: usize,
    _ghost_x_size: usize,
    vec: Vec<Vec<Vec<isize>>>,
}

impl Matrix {
    #[trusted]
    #[requires(0 < z_size)]
    #[requires(0 < y_size)]
    #[requires(0 < x_size)]
    #[ensures(result.z_size() == z_size)]
    #[ensures(result.y_size() == y_size)]
    #[ensures(result.x_size() == x_size)]
    #[ensures(forall(|z: isize, y: isize, x: isize|
                (0 <= x && x < result.x_size() && 0 <= y && y < result.y_size() && 0 <= z && z < result.z_size()) ==>
                result.lookup(z, y, x) == 0))]
    fn new(z_size: isize, y_size: isize, x_size: isize) -> Self {
        Self {
            _ghost_z_size: z_size as usize,
            _ghost_y_size: y_size as usize,
            _ghost_x_size: x_size as usize,
            vec: vec![vec![vec![0; z_size as usize]; y_size as usize]; x_size as usize],
        }
    }

    #[pure]
    #[trusted]
    #[ensures(0 < result)]
    fn x_size(&self) -> isize {
        self._ghost_x_size as isize
    }

    #[pure]
    #[trusted]
    #[ensures(0 < result)]
    fn y_size(&self) -> isize {
        self._ghost_y_size as isize
    }

    #[pure]
    #[trusted]
    #[ensures(0 < result)]
    fn z_size(&self) -> isize {
        self._ghost_z_size as isize
    }

    #[trusted]
    #[requires(0 <= z && z < self.z_size())]
    #[requires(0 <= y && y < self.y_size())]
    #[requires(0 <= x && x < self.x_size())]
    #[ensures(self.z_size() == old(self.z_size()))]
    #[ensures(self.y_size() == old(self.y_size()))]
    #[ensures(self.x_size() == old(self.x_size()))]
    #[ensures(self.lookup(z, y, x) == value)]
    #[ensures(forall(|i: isize, j: isize, k: isize|
        (0 <= k && k < self.z_size() && 0 <= i && i < self.y_size() &&
         0 <= j && j < self.x_size() && !(j == x && i == y && k == z)) ==>
        self.lookup(k, i, j) == old(self.lookup(k, i, j))))]
    fn set(&mut self, z: isize, y: isize, x: isize, value: isize) -> () {
        self.vec[z as usize][y as usize][x as usize] = value
    }

    #[pure]
    #[trusted]
    #[requires(0 <= z && z < self.z_size())]
    #[requires(0 <= y && y < self.y_size())]
    #[requires(0 <= x && x < self.x_size())]
    fn lookup(&self, z: isize, y: isize, x: isize) -> isize {
        self.vec[z as usize][y as usize][x as usize]
    }
}

// Recursive solution

#[trusted]
#[pure]
#[ensures(result == a + b)]
fn add(a: isize, b: isize) -> isize {
    a.checked_add(b).unwrap()
}

#[pure]
#[requires(n <= 1120)]
#[requires(k >= 0 && k <= 14)]
#[requires(p >= -1 && p < primes.len())]
#[requires(primes.len() > 0)]
#[requires(forall(|k: isize| (k >= 0 && k < primes.len()) ==> (primes.lookup(k) >= 2)))]
fn sum_of_different_primes_rec(primes: &VecWrapperI32, n: isize, k: isize, p: isize) -> isize {
    if k == 0 {
        if n == 0 {
            1
        } else {
            0
        }
    } else if n < 0 {
        0
    } else if p == -1 {
        0
    } else {
        let take = sum_of_different_primes_rec(primes, n - primes.lookup(p), k - 1, p - 1);
        let leave = sum_of_different_primes_rec(primes, n, k, p - 1);
        add(take, leave)
    }
}

// DP SOlution
#[requires(size_n > 1 && size_n <= 1121)]
#[requires(size_k > 1 && size_k <= 15)]
#[requires(size_p == primes.len() + 1)]
#[requires(primes.len() > 0 && primes.len() < size_n)]
#[requires(forall(|k: isize| (k >= 0 && k < primes.len()) ==> (primes.lookup(k) >= 2 && primes.lookup(k) < size_n)))]
#[requires(idx_k > 0 && idx_k < size_k)]
#[requires(idx_n >= 0 && idx_n < size_n)]
#[requires(idx_p > 0 && idx_p < size_p)]
#[requires(dp.z_size() == size_k && dp.y_size() == size_n && dp.x_size() == size_p)]
#[requires(forall(|i: isize, j: isize, k: isize| (i >= 0 && i < idx_k && j >= 0 && j < size_n && k >= 0 && k < size_p) ==>  dp.lookup(i, j, k) == sum_of_different_primes_rec(primes, j, i, k - 1)))]
#[requires(forall(|j: isize, k: isize| (j >= 0 && j <  idx_n && k >= 0 && k < size_p) ==>  dp.lookup(idx_k, j, k) == sum_of_different_primes_rec(primes, j, idx_k, k -  1)))]
#[requires(forall(|k: isize| (k >= 0 && k < idx_p) ==>  dp.lookup(idx_k, idx_n, k) == sum_of_different_primes_rec(primes, idx_n, idx_k, k - 1)))]
#[ensures(result == sum_of_different_primes_rec(primes, idx_n, idx_k, idx_p -  1))]
fn helper(
    primes: &VecWrapperI32,
    size_k: isize,
    size_n: isize,
    size_p: isize,
    idx_k: isize,
    idx_n: isize,
    idx_p: isize,
    dp: &Matrix,
) -> isize {
    let mut result = 0;
    assert!(idx_p > 0);
    let idx_prev = idx_n - primes.lookup(idx_p - 1);
    let mut result = 0;

    if idx_prev >= 0 {
        assert!(
            dp.lookup(idx_k - 1, idx_prev, idx_p - 1)
                == sum_of_different_primes_rec(primes, idx_prev, idx_k - 1, idx_p - 2)
        );
        assert!(
            dp.lookup(idx_k, idx_n, idx_p - 1)
                == sum_of_different_primes_rec(primes, idx_n, idx_k, idx_p - 2)
        );
        let take = dp.lookup(idx_k - 1, idx_prev, idx_p - 1);
        let leave = dp.lookup(idx_k, idx_n, idx_p - 1);
        result = add(take, leave);
        assert!(result == sum_of_different_primes_rec(primes, idx_n, idx_k, idx_p - 1));
    } else {
        assert!(sum_of_different_primes_rec(primes, idx_prev, idx_k - 1, idx_p - 2) == 0);
        assert!(
            dp.lookup(idx_k, idx_n, idx_p - 1)
                == sum_of_different_primes_rec(primes, idx_n, idx_k, idx_p - 2)
        );
        let take = 0;
        let leave = dp.lookup(idx_k, idx_n, idx_p - 1);
        result = add(take, leave);
        assert!(result == sum_of_different_primes_rec(primes, idx_n, idx_k, idx_p - 1));
    }
    assert!(result == sum_of_different_primes_rec(primes, idx_n, idx_k, idx_p - 1));
    result
}

#[requires(size_n > 1 && size_n <= 1121)]
#[requires(size_k > 1 && size_k <= 15)]
#[requires(size_p == primes.len() + 1)]
#[requires(primes.len() > 0 && primes.len() < size_n)]
#[requires(forall(|k: isize| (k >= 0 && k < primes.len()) ==> (primes.lookup(k) >= 2 && primes.lookup(k) < size_n)))]
#[requires(idx_k > 0 && idx_k < size_k)]
#[requires(idx_n >= 0 && idx_n < size_n)]
#[requires(idx_p >= 0 && idx_p < size_p)]
#[requires(dp.z_size() == size_k && dp.y_size() == size_n && dp.x_size() == size_p)]
#[requires(forall(|i: isize, j: isize, k: isize| (i >= 0 && i < idx_k && j >= 0 && j < size_n && k >= 0 && k < size_p) ==>  dp.lookup(i, j, k) == sum_of_different_primes_rec(primes, j, i, k - 1)))]
#[requires(forall(|j: isize, k: isize| (j >= 0 && j <  idx_n && k >= 0 && k < size_p) ==>  dp.lookup(idx_k, j, k) == sum_of_different_primes_rec(primes, j, idx_k, k -  1)))]
#[requires(forall(|k: isize| (k >= 0 && k < idx_p) ==>  dp.lookup(idx_k, idx_n, k) == sum_of_different_primes_rec(primes, idx_n, idx_k, k - 1)))]
#[ensures(result == sum_of_different_primes_rec(primes, idx_n, idx_k, idx_p -  1))]
fn helper2(
    primes: &VecWrapperI32,
    size_k: isize,
    size_n: isize,
    size_p: isize,
    idx_k: isize,
    idx_n: isize,
    idx_p: isize,
    dp: &Matrix,
) -> isize {
    if idx_p == 0 {
        0
    } else {
        helper(primes, size_k, size_n, size_p, idx_k, idx_n, idx_p, dp)
    }
}

#[requires(n > 0 && n <= 1120)]
#[requires(k > 0 && k <= 14)]
#[requires(primes.len() > 0 && primes.len() <= n)]
#[requires(forall(|k: isize| (k >= 0 && k < primes.len()) ==> (primes.lookup(k) >= 2 && primes.lookup(k) <= n)))]
#[ensures(result == sum_of_different_primes_rec(primes, n, k, primes.len() - 1))]
fn sum_of_different_primes(primes: &VecWrapperI32, n: isize, k: isize) -> isize {
    let size_k = k + 1;
    let size_n = n + 1;
    let primes_len = primes.len();
    let size_p = primes_len + 1;
    let mut dp = Matrix::new(size_k, size_n, size_p);
    let mut idx_n = 0isize;
    while idx_n < size_n {
        body_invariant!(idx_n >= 0 && idx_n < dp.y_size());
        body_invariant!(dp.z_size() == size_k && dp.y_size() == size_n && dp.x_size() == size_p);
        body_invariant!(forall(|j: isize, k: isize| (j >= 0 && j < idx_n && k >= 0 && k < size_p) ==>  dp.lookup(0, j, k) == sum_of_different_primes_rec(primes, j, 0, k - 1)));
        let mut idx_p = 0isize;
        while idx_p < size_p {
            body_invariant!(idx_p >= 0 && idx_p < dp.x_size());
            body_invariant!(
                dp.z_size() == size_k && dp.y_size() == size_n && dp.x_size() == size_p
            );
            body_invariant!(forall(|j: isize, k: isize| (j >= 0 && j < idx_n && k >= 0 && k < size_p) ==>  dp.lookup(0, j, k) == sum_of_different_primes_rec(primes, j, 0, k - 1)));
            body_invariant!(forall(|k: isize| (k >= 0 && k < idx_p) ==>  dp.lookup(0, idx_n, k) == sum_of_different_primes_rec(primes, idx_n, 0, k - 1)));
            let one = 1isize;
            let zero = 0isize;
            if idx_n == 0 {
                dp.set(zero, idx_n, idx_p, one);
            } else {
                dp.set(zero, idx_n, idx_p, zero);
            }
            idx_p += 1;
        }
        idx_n += 1
    }

    let mut idx_k = 1isize;
    while idx_k < size_k {
        body_invariant!(idx_k >= 0 && idx_k < size_k);
        body_invariant!(dp.z_size() == size_k && dp.y_size() == size_n && dp.x_size() == size_p);
        body_invariant!(idx_k > 0);
        body_invariant!(forall(|i: isize, j: isize, k: isize| (i >= 0 && i < idx_k && j >= 0 && j <=  n && k >= 0 && k < size_p) ==>  dp.lookup(i, j, k) == sum_of_different_primes_rec(primes, j, i, k - 1)));
        let mut idx_n = 0isize;
        while idx_n < size_n {
            body_invariant!(idx_n >= 0 && idx_n < size_n);
            body_invariant!(
                dp.z_size() == size_k && dp.y_size() == size_n && dp.x_size() == size_p
            );
            body_invariant!(forall(|i: isize, j: isize, k: isize| (i >= 0 && i < idx_k && j >= 0 && j <=  n && k >= 0 && k < size_p) ==>  dp.lookup(i, j, k) == sum_of_different_primes_rec(primes, j, i, k - 1)));
            body_invariant!(forall(|j: isize, k: isize| (j >= 0 && j < idx_n && k >= 0 && k < size_p) ==>  dp.lookup(idx_k, j, k) == sum_of_different_primes_rec(primes, j, idx_k, k - 1)));
            let mut idx_p = 0isize;
            while idx_p < size_p {
                body_invariant!(idx_p >= 0 && idx_p < size_p);
                body_invariant!(
                    dp.z_size() == size_k && dp.y_size() == size_n && dp.x_size() == size_p
                );
                body_invariant!(forall(|i: isize, j: isize, k: isize| (i >= 0 && i < idx_k && j >= 0 && j <= n && k >= 0 && k < size_p) ==>  dp.lookup(i, j, k) == sum_of_different_primes_rec(primes, j, i, k - 1)));
                body_invariant!(forall(|j: isize, k: isize| (j >= 0 && j <  idx_n && k >= 0 && k < size_p) ==>  dp.lookup(idx_k, j, k) == sum_of_different_primes_rec(primes, j, idx_k, k -  1)));
                body_invariant!(forall(|k: isize| (k >= 0 && k < idx_p) ==>  dp.lookup(idx_k, idx_n, k) == sum_of_different_primes_rec(primes, idx_n, idx_k, k - 1)));
                let mut result = helper2(primes, size_k, size_n, size_p, idx_k, idx_n, idx_p, &dp);
                dp.set(idx_k, idx_n, idx_p, result);
                idx_p += 1;
            }
            idx_n += 1;
        }
        idx_k += 1;
    }
    dp.lookup(k, n, primes_len)
}





fn main() {}
