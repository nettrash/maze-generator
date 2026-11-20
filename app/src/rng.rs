//! The deterministic random number generator the algorithms draw from, and
//! where seeds come from.

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
///
/// The seed is below 2^53, the largest integer a JSON reader that stores
/// numbers as doubles (JavaScript's, among others) reads back exactly. The
/// seed is reported so that a maze can be replayed, and a seed rounded on its
/// way through `--json` replays a different maze; a full 64-bit seed was
/// above that limit almost every time. Any `u64` still works as a seed.
pub fn random_seed() -> u64 {
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545_F491_4F6C_DD1D);
    let counter = SEED_COUNTER.fetch_add(1, Ordering::Relaxed);
    // Splitmix64 of the sum, so neighbouring counter values spread apart
    // instead of differing by one, keeping the top 53 bits.
    mix(clock
        .wrapping_add(counter.wrapping_mul(GOLDEN_GAMMA))
        .wrapping_add(GOLDEN_GAMMA))
        >> (64 - SEED_BITS)
}

/// How many bits a seed from [`random_seed`] has; see there.
pub const SEED_BITS: u32 = 53;

/// Derives the seed of an independent stream of draws from `seed`.
///
/// Anything that needs randomness beside the algorithm itself, such as the
/// braiding of a finished maze or the CLI's `--algorithm random` pick, has to
/// draw from somewhere the algorithm does not, or its choices repeat the
/// algorithm's. Offsetting the seed does not achieve that with splitmix64:
/// `Rng::new(seed + k * 0x9E37_79B9_7F4A_7C15)` is exactly `Rng::new(seed)`
/// advanced by `k` draws. Mixing the seed with a constant per purpose instead
/// lands on a starting point with no simple relation to the original stream,
/// so the two share no draws however many each takes.
///
/// The same `seed` and `stream` always give the same result, which keeps a
/// maze that uses a derived stream reproducible from its seed.
pub fn derive_seed(seed: u64, stream: u64) -> u64 {
    // The key is forced odd so that it is never zero: stream 0 would
    // otherwise key with `mix(0)`, which is 0, and seed 0, which `mix` also
    // leaves alone, would derive itself and share every draw.
    mix(seed ^ (mix(stream) | 1))
}

/// The splitmix64 increment, the odd number closest to 2^64 divided by the
/// golden ratio.
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// The splitmix64 output function: scrambles `z` so that inputs differing in
/// a single bit give unrelated outputs.
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Small, fast, deterministic pseudo-random generator (splitmix64).
///
/// It is seeded either from the system clock ([`Rng::from_entropy`]) for real
/// runs or from a fixed seed ([`Rng::new`]) so algorithm output can be made
/// reproducible in tests.
#[derive(Debug, Clone)]
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
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        mix(self.state)
    }

    /// Returns a value in `0..n` (or `0` when `n == 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    /// The same value as [`Rng::below`], from the same single draw, for the
    /// small counts the random walks ask for.
    ///
    /// Aldous-Broder and Wilson pick a neighbour at every step of their walks,
    /// billions of times on a large maze, and dividing by a count only known
    /// at run time was a large share of each step. A cell has at most four
    /// neighbours, and for those counts the remainder is a mask or a division
    /// by a constant, which the compiler turns into a multiplication.
    pub(crate) fn below_small(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        let value = self.next_u64();
        (match n {
            1 => 0,
            2 => value & 1,
            3 => value % 3,
            4 => value & 3,
            _ => value % n as u64,
        }) as usize
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
    use super::{GOLDEN_GAMMA, Rng, SEED_BITS, derive_seed, random_seed};
    use std::collections::HashSet;

    /// Factoring the finaliser out of `next_u64` must not have moved the
    /// stream: these are the first outputs of the splitmix64 reference
    /// implementation seeded with zero.
    #[test]
    fn the_stream_is_still_splitmix64() {
        let mut rng = Rng::new(0);
        assert_eq!(rng.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(rng.next_u64(), 0x6e78_9e6a_a1b9_65f4);
        assert_eq!(rng.next_u64(), 0x06c4_5d18_8009_454f);
    }

    /// Braiding once seeded itself with `seed + GOLDEN_GAMMA`, which only
    /// advances the same splitmix64 stream: it repeated the algorithm's
    /// draws one step later. A derived stream must share none of them.
    #[test]
    fn a_derived_stream_shares_no_draws_with_its_parent() {
        for seed in [0, 1, 42, u64::MAX, GOLDEN_GAMMA] {
            let mut parent = Rng::new(seed);
            let drawn: HashSet<u64> = (0..512).map(|_| parent.next_u64()).collect();
            for stream in [0, 1, 2, u64::from_le_bytes(*b"braiding")] {
                let mut child = Rng::new(derive_seed(seed, stream));
                for _ in 0..512 {
                    assert!(
                        !drawn.contains(&child.next_u64()),
                        "stream {stream} of seed {seed} repeats its parent"
                    );
                }
            }
            // The old offset fails this very check on its first draw.
            let mut offset = Rng::new(seed.wrapping_add(GOLDEN_GAMMA));
            assert!(drawn.contains(&offset.next_u64()));
        }
    }

    #[test]
    fn derived_seeds_are_reproducible_and_depend_on_both_inputs() {
        assert_eq!(derive_seed(7, 1), derive_seed(7, 1));
        assert_ne!(derive_seed(7, 1), derive_seed(7, 2));
        assert_ne!(derive_seed(7, 1), derive_seed(8, 1));
        assert_ne!(derive_seed(7, 1), 7);
    }

    #[test]
    fn random_seeds_survive_a_trip_through_a_json_double() {
        for _ in 0..1000 {
            let seed = random_seed();
            assert!(seed < 1 << SEED_BITS, "{seed} is not exact as a double");
            assert_eq!(seed as f64 as u64, seed);
        }
    }

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
    fn below_small_draws_exactly_what_below_draws() {
        for seed in [0, 1, 7, u64::MAX] {
            let (mut fast, mut plain) = (Rng::new(seed), Rng::new(seed));
            for round in 0..2_000 {
                let n = round % 7;
                assert_eq!(
                    fast.below_small(n),
                    plain.below(n as u64) as usize,
                    "seed {seed}, round {round}, n {n}"
                );
            }
        }
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
