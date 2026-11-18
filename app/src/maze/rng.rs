use std::time::{SystemTime, UNIX_EPOCH};

/// Small, fast, deterministic pseudo-random generator (splitmix64).
///
/// It is seeded either from the system clock ([`Rng::from_entropy`]) for real
/// runs or from a fixed seed ([`Rng::new`]) so algorithm output can be made
/// reproducible in tests.
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Creates a generator from an explicit seed.
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    /// Creates a generator seeded from the current system time.
    pub fn from_entropy() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545_F491_4F6C_DD1D);
        Rng::new(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a value in `0..n` (or `0` when `n == 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    /// Returns a random boolean.
    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Shuffles `items` in place (Fisher-Yates).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn deterministic_for_same_seed() {
        let a: Vec<u64> = (0..10).map(|_| Rng::new(7).below(100)).collect();
        assert!(a.iter().all(|&v| v == a[0]));
    }

    #[test]
    fn below_respects_bound() {
        let mut r = Rng::new(123);
        for _ in 0..1000 {
            assert!(r.below(6) < 6);
        }
        assert_eq!(r.below(0), 0);
    }

    #[test]
    fn shuffle_is_permutation() {
        let mut r = Rng::new(99);
        let mut v: Vec<u32> = (0..50).collect();
        r.shuffle(&mut v);
        v.sort_unstable();
        assert_eq!(v, (0..50).collect::<Vec<_>>());
    }
}
