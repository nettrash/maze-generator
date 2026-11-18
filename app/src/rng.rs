use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Mixed into the clock so two seeds drawn in the same tick still differ.
static SEED_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Draws a seed from the system clock.
///
/// Split out from [`Rng::from_entropy`] so a caller that has to hand the same
/// seed to several generators (the CLI does this to report it afterwards) can
/// pick it once.
///
/// A counter is mixed with the clock because the clock is not always precise
/// enough to distinguish two calls made back to back, and two runs that shared
/// a seed would silently produce the same maze.
pub fn random_seed() -> u64 {
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545_F491_4F6C_DD1D);
    let counter = SEED_COUNTER.fetch_add(1, Ordering::Relaxed);
    // Splitmix64 of the sum, so neighbouring counter values spread apart
    // instead of differing by one.
    let mut z = clock
        .wrapping_add(counter.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Small, fast, deterministic pseudo-random generator (splitmix64).
///
/// It is seeded either from the system clock ([`Rng::from_entropy`]) for real
/// runs or from a fixed seed ([`Rng::new`]) so algorithm output can be made
/// reproducible in tests.
pub struct Rng {
    seed: u64,
    state: u64,
}

impl Rng {
    /// Creates a generator from an explicit seed.
    pub fn new(seed: u64) -> Self {
        Rng { seed, state: seed }
    }

    /// Creates a generator seeded from the current system time.
    pub fn from_entropy() -> Self {
        Rng::new(random_seed())
    }

    /// The seed this generator started from.
    ///
    /// Reporting it lets a run be replayed exactly with [`Rng::new`].
    pub fn seed(&self) -> u64 {
        self.seed
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
    use super::{Rng, random_seed};

    #[test]
    fn random_seeds_do_not_repeat() {
        let seeds: Vec<u64> = (0..1000).map(|_| random_seed()).collect();
        let unique: std::collections::HashSet<u64> = seeds.iter().copied().collect();
        assert_eq!(unique.len(), seeds.len(), "two runs shared a seed");
    }

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
