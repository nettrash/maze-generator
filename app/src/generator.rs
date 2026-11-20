//! Picks an algorithm by name, checks the size, and builds the maze.

use std::fmt;

use crate::{
    algorithm::{
        abstraction::AlgorithmAbstraction, aldous_broder::AldousBroderAlgorithm,
        backtracking::BacktrackingAlgorithm, eller::EllerAlgorithm, kruskal::KruskalAlgorithm,
        prim::PrimAlgorithm, types::AlgorithmType, wilson::WilsonAlgorithm,
    },
    error::MazeError,
    map::Map,
    rng::{Rng, derive_seed},
};

/// The algorithm names accepted by [`Generator::new`] and by the `--algorithm`
/// command line option, in the order they are advertised to the user.
///
/// Spelled out from [`AlgorithmType::ALL`], so the list cannot drift from the
/// algorithms there are.
pub const ALGORITHMS: [&str; AlgorithmType::ALL.len()] = {
    let mut names = [""; AlgorithmType::ALL.len()];
    let mut index = 0;
    while index < names.len() {
        names[index] = AlgorithmType::ALL[index].as_str();
        index += 1;
    }
    names
};

/// The [`derive_seed`] stream that braiding draws from.
const BRAID_STREAM: u64 = u64::from_le_bytes(*b"braiding");

/// Variants applied to a maze after the algorithm has finished.
///
/// Both default to `false`, which leaves the plain perfect maze the algorithms
/// produce on their own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MazeOptions {
    /// Knock a wall down at every dead end, so the maze has loops and no
    /// cul-de-sacs. This is what is usually called a *braided* maze.
    pub braid: bool,
    /// Open the outer border at the entrance and the exit instead of keeping
    /// the maze fully enclosed.
    pub entrance_exit: bool,
}

impl MazeOptions {
    /// Whether no variant was asked for, which leaves the plain perfect maze.
    pub fn is_plain(&self) -> bool {
        !self.braid && !self.entrance_exit
    }
}

/// The largest number of cells a [`Generator`] will build.
///
/// A maze is `width * height` cells, so the cost of a request grows with the
/// *square* of the side length and nothing in the arguments stops someone
/// asking for a million by a million. At 5000x5000 the slowest algorithm,
/// Aldous-Broder, takes about 13 seconds and the grid alone is 25 MB; at
/// 100000x100000 it is 10 GB of grid and would not finish. The limit turns
/// that into an immediate, explained error instead of a hung process.
///
/// 25 million cells is 5000x5000, comfortably above anything a person prints,
/// renders or saves to look at, and still only 25 MB of grid.
pub const MAX_CELLS: u64 = 25_000_000;

/// The longest side `aldous_broder` and `wilson` take, unless the maze is a
/// single row or column.
///
/// Both build the maze from random walks, and a random walk needs about the
/// square of a distance to cover it, so their running time grows with the
/// square of the longer side rather than with the number of cells. Within
/// [`MAX_CELLS`] that let a request run for days: a 10x2500000 Aldous-Broder
/// maze would have taken about 44 hours, a 2x1000000 one about five. At this
/// limit the slowest shape left, 1250x20000, takes Aldous-Broder about 20
/// seconds, close to the 13 a 5000x5000 maze takes. A maze one cell wide has
/// a single perfect maze, the corridor, which is carved without walking, so
/// it is exempt.
pub const MAX_WALK_SIDE: u64 = 20_000;

/// Checks a size against every limit [`Generator::new`] applies to
/// `algorithm`, without building anything.
///
/// This is how to tell in advance which algorithms can build a shape: the
/// command line uses it so that `--algorithm random` only picks one that can.
pub fn check_size(algorithm: AlgorithmType, width: u64, height: u64) -> Result<(), MazeError> {
    check_cells(width, height)?;
    check_walk(algorithm, width, height)
}

/// Rejects a zero dimension and a maze of more than [`MAX_CELLS`] cells.
fn check_cells(width: u64, height: u64) -> Result<(), MazeError> {
    if width == 0 || height == 0 {
        return Err(MazeError::EmptySize { width, height });
    }
    // Saturating so that width * height cannot overflow and slip a huge
    // request past the limit.
    if width.saturating_mul(height) > MAX_CELLS {
        return Err(MazeError::TooLarge {
            width,
            height,
            limit: MAX_CELLS,
        });
    }
    Ok(())
}

/// Rejects a random-walk algorithm on a shape longer than [`MAX_WALK_SIDE`].
fn check_walk(algorithm: AlgorithmType, width: u64, height: u64) -> Result<(), MazeError> {
    let walks = matches!(
        algorithm,
        AlgorithmType::AldousBroder | AlgorithmType::Wilson
    );
    if walks && width.min(height) > 1 && width.max(height) > MAX_WALK_SIDE {
        return Err(MazeError::TooLong {
            algorithm: algorithm.as_str().to_string(),
            width,
            height,
            limit: MAX_WALK_SIDE,
        });
    }
    Ok(())
}

/// Builds mazes of one size with one algorithm.
///
/// The fields are private so that a size [`Generator::new`] has checked
/// cannot be changed afterwards: with public fields, setting `width` to 0
/// or past [`MAX_CELLS`] undid both checks.
pub struct Generator {
    width: u64,
    height: u64,
    algorithm: Box<dyn AlgorithmAbstraction>,
    options: MazeOptions,
}

impl fmt::Debug for Generator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Generator")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("algorithm", &self.algorithm.get_algorithm_type())
            .field("seed", &self.seed())
            .field("options", &self.options)
            .finish()
    }
}

impl Generator {
    /// Builds a generator for the named algorithm.
    ///
    /// When `seed` is `Some`, the algorithm is seeded with it and produces the
    /// same maze on every run; when `None`, it is seeded from the system clock.
    /// The seed actually used is available from [`Generator::seed`], so a maze
    /// the user likes can be replayed.
    ///
    /// Returns [`MazeError::UnsupportedAlgorithm`] when `algorithm` is not one
    /// of [`ALGORITHMS`], so a mistyped name is reported to the user instead of
    /// aborting the process.
    ///
    /// Rejects a zero dimension with [`MazeError::EmptySize`] and a maze larger
    /// than [`MAX_CELLS`] with [`MazeError::TooLarge`]. Both used to be
    /// accepted: `--width 0` reported a maze as generated and printed nothing,
    /// and a huge size simply ran until it exhausted memory. A random-walk
    /// algorithm on a shape longer than [`MAX_WALK_SIDE`] is rejected with
    /// [`MazeError::TooLong`], since its running time grows with the square of
    /// the longer side.
    pub fn new(
        width: u64,
        height: u64,
        algorithm: &str,
        seed: Option<u64>,
    ) -> Result<Self, MazeError> {
        check_cells(width, height)?;
        let kind: AlgorithmType = algorithm.parse()?;
        check_walk(kind, width, height)?;
        // A match on the type rather than on the name, so a new algorithm
        // cannot be listed without being dispatched: the match stops
        // compiling until it is.
        let alg: Box<dyn AlgorithmAbstraction> = match kind {
            AlgorithmType::Backtracking => {
                Box::new(BacktrackingAlgorithm::with_seed_or_random(seed))
            }
            AlgorithmType::Prim => Box::new(PrimAlgorithm::with_seed_or_random(seed)),
            AlgorithmType::Kruskal => Box::new(KruskalAlgorithm::with_seed_or_random(seed)),
            AlgorithmType::AldousBroder => {
                Box::new(AldousBroderAlgorithm::with_seed_or_random(seed))
            }
            AlgorithmType::Wilson => Box::new(WilsonAlgorithm::with_seed_or_random(seed)),
            AlgorithmType::Eller => Box::new(EllerAlgorithm::with_seed_or_random(seed)),
        };
        Ok(Generator {
            width,
            height,
            algorithm: alg,
            options: MazeOptions::default(),
        })
    }

    /// The width of the mazes this generator builds, in cells.
    pub fn width(&self) -> u64 {
        self.width
    }

    /// The height of the mazes this generator builds, in cells.
    pub fn height(&self) -> u64 {
        self.height
    }

    /// The algorithm this generator runs.
    pub fn algorithm(&self) -> &dyn AlgorithmAbstraction {
        self.algorithm.as_ref()
    }

    /// Asks for maze variants on top of what the algorithm produces.
    pub fn with_options(mut self, options: MazeOptions) -> Self {
        self.options = options;
        self
    }

    /// The variants this generator applies after the algorithm has run.
    pub fn options(&self) -> MazeOptions {
        self.options
    }

    /// The seed the algorithm will use, useful for reproducing a maze.
    pub fn seed(&self) -> u64 {
        self.algorithm.seed()
    }

    /// Runs the algorithm and applies the requested variants.
    ///
    /// This is the entry point for callers that want the maze and nothing else.
    /// Generation is CPU-bound and synchronous, so there is no runtime to
    /// arrange: an earlier `async fn generate()` wrapped this call and forced
    /// every consumer onto a tokio runtime for nothing.
    /// It prints nothing: a library that writes to stdout corrupts the output
    /// of whoever uses it, so progress reporting belongs to the command line
    /// binary, which has [`Generator::seed`] and the sizes to report.
    ///
    /// Braiding draws its choices from the generator's seed, so a braided maze
    /// is just as reproducible as a plain one.
    pub fn build(&self) -> Map {
        let mut map = self.algorithm.generate_maze(self.width, self.height);
        if self.options.entrance_exit {
            map.open_entrance_and_exit();
        }
        if self.options.braid {
            // A derived stream rather than the algorithm's own, so the braid
            // choices do not repeat the draws the algorithm just made.
            map.braid(&mut Rng::new(derive_seed(self.seed(), BRAID_STREAM)));
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::{ALGORITHMS, Generator, MAX_CELLS, MAX_WALK_SIDE, check_size};
    use crate::{AlgorithmType, MazeError};

    /// Every algorithm must return a perfect, solvable maze of the requested
    /// size, including the degenerate one-cell and single-row shapes.
    #[test]
    fn all_algorithms_generate_perfect_mazes() {
        let sizes = [(1u64, 1u64), (1, 7), (6, 1), (9, 6), (12, 10)];

        for name in ALGORITHMS {
            for (width, height) in sizes {
                let generator = Generator::new(width, height, name, None).expect("known algorithm");
                let map = generator.build();
                assert_eq!(map.width(), width, "{name} width at {width}x{height}");
                assert_eq!(map.height(), height, "{name} height at {width}x{height}");
                assert!(
                    map.is_perfect(),
                    "{name} did not produce a perfect maze at {width}x{height}"
                );
                assert!(
                    map.is_solvable(),
                    "{name} produced an unsolvable maze at {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn dispatches_the_requested_algorithm() {
        for name in ALGORITHMS {
            let generator = Generator::new(3, 3, name, None).expect("known algorithm");
            assert_eq!(generator.algorithm().get_algorithm_type().as_str(), name);
        }
    }

    #[test]
    fn generator_reports_the_seed_it_will_use() {
        let generator = Generator::new(3, 3, "prim", Some(1234)).expect("known algorithm");
        assert_eq!(generator.seed(), 1234);
    }

    #[test]
    fn an_unseeded_generator_uses_a_fresh_seed_each_time() {
        let first = Generator::new(12, 9, "prim", None)
            .expect("known algorithm")
            .seed();
        let second = Generator::new(12, 9, "prim", None)
            .expect("known algorithm")
            .seed();
        assert_ne!(first, second, "two unseeded runs should differ");
    }

    /// Reproducibility is the whole point of `--seed`, so it is checked for
    /// every algorithm rather than just one.
    #[test]
    fn every_algorithm_reproduces_itself_from_its_seed() {
        for name in ALGORITHMS {
            let first = Generator::new(12, 9, name, Some(7))
                .expect("known algorithm")
                .build();
            let second = Generator::new(12, 9, name, Some(7))
                .expect("known algorithm")
                .build();
            assert_eq!(
                first.to_ascii(),
                second.to_ascii(),
                "{name} is not reproducible from its seed"
            );
        }
    }

    #[test]
    fn a_different_seed_produces_a_different_maze() {
        for name in ALGORITHMS {
            let first = Generator::new(12, 9, name, Some(7))
                .expect("known algorithm")
                .build();
            let second = Generator::new(12, 9, name, Some(8))
                .expect("known algorithm")
                .build();
            assert_ne!(
                first.to_ascii(),
                second.to_ascii(),
                "{name} ignores its seed"
            );
        }
    }

    #[test]
    fn unknown_algorithm_is_an_error_not_a_panic() {
        let result = Generator::new(3, 3, "not-an-algorithm", None);
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("an unknown algorithm must not build a generator"),
        };
        let message = error.to_string();
        assert!(
            message.contains("not-an-algorithm"),
            "the message should name the bad input: {message}"
        );
        for name in ALGORITHMS {
            assert!(
                message.contains(name),
                "the message should list the valid choices: {message}"
            );
        }
    }

    /// A zero dimension used to be accepted: the tool printed "Generating a
    /// 0x5 maze", printed an empty maze, and exited 0.
    #[test]
    fn a_zero_dimension_is_rejected() {
        for (width, height) in [(0, 0), (0, 1), (1, 0), (0, 100), (100, 0)] {
            let error = match Generator::new(width, height, "prim", Some(1)) {
                Err(error) => error,
                Ok(_) => panic!("{width}x{height} should not build a generator"),
            };
            assert!(
                matches!(error, MazeError::EmptySize { .. }),
                "{width}x{height} gave {error}"
            );
            let message = error.to_string();
            assert!(
                message.contains("positive width and height")
                    && message.contains(&format!("got {width}x{height}")),
                "the message should explain the rule and show the size: {message}"
            );
        }
    }

    /// A huge size used to run until memory ran out. 100000x100000 is 10 GB of
    /// grid, so it has to be refused up front rather than discovered by the
    /// person who ran it.
    #[test]
    fn an_absurd_size_is_rejected_before_any_work() {
        for (width, height) in [
            (100_000, 100_000),
            (MAX_CELLS + 1, 1),
            (1, MAX_CELLS + 1),
            (u64::MAX, u64::MAX),
            (1 << 40, 1 << 40),
        ] {
            let error = match Generator::new(width, height, "prim", Some(1)) {
                Err(error) => error,
                Ok(_) => panic!("{width}x{height} should not build a generator"),
            };
            assert!(
                matches!(error, MazeError::TooLarge { .. }),
                "{width}x{height} gave {error}"
            );
        }
    }

    /// The limit is a cell count, not a side length, and the multiplication
    /// must not overflow a large-but-legal request into passing it.
    #[test]
    fn the_size_limit_counts_cells() {
        // Exactly at the limit is allowed: 5000x5000 is 25 million cells.
        assert_eq!(5000 * 5000, MAX_CELLS);
        assert!(Generator::new(5000, 5000, "prim", Some(1)).is_ok());
        // One cell over is not.
        let error = match Generator::new(5000, 5001, "prim", Some(1)) {
            Ok(_) => panic!("one cell over the limit should be refused"),
            Err(error) => error,
        };
        assert!(matches!(error, MazeError::TooLarge { .. }));
        assert!(
            error.to_string().contains("25005000"),
            "the message should report the cell count: {error}"
        );
    }

    /// A random walk needs about the square of the longer side, so a legal
    /// 10x2500000 Aldous-Broder maze would have run for about 44 hours.
    #[test]
    fn random_walks_refuse_long_thin_shapes() {
        for name in ["aldous_broder", "wilson"] {
            for (width, height) in [
                (10, 2_500_000),
                (2, MAX_WALK_SIDE + 1),
                (MAX_WALK_SIDE + 1, 2),
            ] {
                let error = Generator::new(width, height, name, Some(1))
                    .expect_err("a shape this long would take hours");
                assert!(
                    matches!(error, MazeError::TooLong { .. }),
                    "{name} {width}x{height} gave {error}"
                );
                assert!(error.to_string().contains(name), "{error}");
            }
            // The limit itself is accepted, and so is a corridor of any length.
            assert!(Generator::new(2, MAX_WALK_SIDE, name, Some(1)).is_ok());
            assert!(Generator::new(1, MAX_CELLS, name, Some(1)).is_ok());
            assert!(Generator::new(MAX_CELLS, 1, name, Some(1)).is_ok());
        }
        // The other algorithms take any shape within MAX_CELLS.
        for name in ["backtracking", "prim", "kruskal", "eller"] {
            assert!(
                Generator::new(10, 2_500_000, name, Some(1)).is_ok(),
                "{name}"
            );
        }
    }

    #[test]
    fn check_size_agrees_with_new() {
        for kind in AlgorithmType::ALL {
            for (width, height) in [
                (0, 5),
                (5000, 5001),
                (10, 2_500_000),
                (1, MAX_CELLS),
                (40, 25),
            ] {
                assert_eq!(
                    check_size(kind, width, height).is_ok(),
                    Generator::new(width, height, kind.as_str(), Some(1)).is_ok(),
                    "{kind} {width}x{height}"
                );
            }
        }
    }

    /// A corridor one cell wide used to be walked one step at a time, which
    /// takes the random walks about the square of its length: days at 25
    /// million cells. They carve it directly now, so a million cells take a
    /// fraction of a second. The bound below is a minute, generous enough for
    /// any loaded machine, and it turns a lost shortcut into a failure rather
    /// than a test that never ends.
    #[test]
    fn the_random_walks_carve_a_long_corridor_at_once() {
        for name in ["aldous_broder", "wilson"] {
            for (width, height) in [(1, 1_000_000), (1_000_000, 1)] {
                let generator =
                    Generator::new(width, height, name, Some(1)).expect("known algorithm");
                let (done, finished) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = done.send(generator.build().is_perfect());
                });
                let perfect = finished
                    .recv_timeout(std::time::Duration::from_secs(60))
                    .unwrap_or_else(|_| panic!("{name} walked a {width}x{height} corridor"));
                assert!(perfect, "{name} {width}x{height}");
            }
        }
    }

    /// Every algorithm draws the same corridor, the only perfect maze one cell
    /// wide.
    #[test]
    fn every_algorithm_draws_the_same_corridor() {
        for (width, height) in [(1, 9), (9, 1)] {
            let corridors: Vec<String> = ALGORITHMS
                .iter()
                .map(|name| {
                    Generator::new(width, height, name, Some(3))
                        .expect("known algorithm")
                        .build()
                        .to_ascii()
                })
                .collect();
            assert!(
                corridors.windows(2).all(|pair| pair[0] == pair[1]),
                "{corridors:#?}"
            );
        }
    }

    #[test]
    fn algorithm_names_are_matched_exactly() {
        // Names are matched case and whitespace sensitively, so the CLI help
        // text and the accepted values stay in lockstep.
        for bad in ["Backtracking", " prim", "prim ", "primus", ""] {
            assert!(
                Generator::new(3, 3, bad, None).is_err(),
                "{bad:?} should not be accepted"
            );
        }
    }
}
