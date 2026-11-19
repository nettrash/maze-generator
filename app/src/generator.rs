use crate::{
    algorithm::{
        abstraction::AlgorithmAbstraction, aldous_broder::AldousBroderAlgorithm,
        backtracking::BacktrackingAlgorithm, eller::EllerAlgorithm, kruskal::KruskalAlgorithm,
        prim::PrimAlgorithm, wilson::WilsonAlgorithm,
    },
    error::MazeError,
    map::Map,
    rng::Rng,
};

/// The algorithm names accepted by [`Generator::new`] and by the `--algorithm`
/// command line option, in the order they are advertised to the user.
pub const ALGORITHMS: [&str; 6] = [
    "backtracking",
    "prim",
    "kruskal",
    "aldous_broder",
    "wilson",
    "eller",
];

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
    /// Whether any variant was asked for.
    pub fn is_plain(&self) -> bool {
        !self.braid && !self.entrance_exit
    }
}

/// The largest number of cells a [`Generator`] will build.
///
/// A maze is `width * height` cells, so the cost of a request grows with the
/// *square* of the side length and nothing in the arguments stops someone
/// asking for a million by a million. At 5000x5000 the slowest algorithm takes
/// about 17 seconds and the grid alone is 25 MB; at 100000x100000 it is 10 GB
/// of grid and would not finish. The limit turns that into an immediate,
/// explained error instead of a hung process.
///
/// 25 million cells is 5000x5000, comfortably above anything a person prints,
/// renders or saves to look at, and still only 25 MB of grid.
pub const MAX_CELLS: u64 = 25_000_000;

pub struct Generator {
    pub width: u64,
    pub height: u64,
    pub algorithm: Box<dyn AlgorithmAbstraction>,
    options: MazeOptions,
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
    /// and a huge size simply ran until it exhausted memory.
    pub fn new(
        width: u64,
        height: u64,
        algorithm: &str,
        seed: Option<u64>,
    ) -> Result<Self, MazeError> {
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
        let alg: Box<dyn AlgorithmAbstraction> = match algorithm {
            "backtracking" => Box::new(BacktrackingAlgorithm::with_seed_or_random(seed)),
            "prim" => Box::new(PrimAlgorithm::with_seed_or_random(seed)),
            "kruskal" => Box::new(KruskalAlgorithm::with_seed_or_random(seed)),
            "aldous_broder" => Box::new(AldousBroderAlgorithm::with_seed_or_random(seed)),
            "wilson" => Box::new(WilsonAlgorithm::with_seed_or_random(seed)),
            "eller" => Box::new(EllerAlgorithm::with_seed_or_random(seed)),
            _ => {
                return Err(MazeError::UnsupportedAlgorithm {
                    name: algorithm.to_string(),
                });
            }
        };
        Ok(Generator {
            width,
            height,
            algorithm: alg,
            options: MazeOptions::default(),
        })
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
    /// Kept for callers that already await; it is [`Generator::build`] under
    /// the hood and has never needed a runtime. New callers should use
    /// [`Generator::build`], which says so in its signature.
    pub async fn generate(&self) -> Result<Map, MazeError> {
        Ok(self.build())
    }

    /// Runs the algorithm and applies the requested variants.
    ///
    /// This is the entry point for callers that want the maze and nothing else.
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
            // A derived seed rather than the algorithm's own, so the braid
            // choices do not repeat the sequence the algorithm just drew from.
            let derived = self.seed().wrapping_add(0x9E37_79B9_7F4A_7C15);
            map.braid(&mut Rng::new(derived));
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::MazeError;
    use super::{ALGORITHMS, Generator, MAX_CELLS};

    /// Every algorithm must return a perfect, solvable maze of the requested
    /// size, including the degenerate one-cell and single-row shapes.
    #[tokio::test]
    async fn all_algorithms_generate_perfect_mazes() {
        let sizes = [(1u64, 1u64), (1, 7), (6, 1), (9, 6), (12, 10)];

        for name in ALGORITHMS {
            for (width, height) in sizes {
                let generator = Generator::new(width, height, name, None).expect("known algorithm");
                let map = generator
                    .generate()
                    .await
                    .expect("generation should not fail");
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
            assert_eq!(generator.algorithm.get_algorithm_type().as_string(), name);
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
    #[tokio::test]
    async fn every_algorithm_reproduces_itself_from_its_seed() {
        for name in ALGORITHMS {
            let first = Generator::new(12, 9, name, Some(7))
                .expect("known algorithm")
                .generate()
                .await
                .expect("generation succeeds");
            let second = Generator::new(12, 9, name, Some(7))
                .expect("known algorithm")
                .generate()
                .await
                .expect("generation succeeds");
            assert_eq!(
                first.to_ascii(),
                second.to_ascii(),
                "{name} is not reproducible from its seed"
            );
        }
    }

    #[tokio::test]
    async fn a_different_seed_produces_a_different_maze() {
        for name in ALGORITHMS {
            let first = Generator::new(12, 9, name, Some(7))
                .expect("known algorithm")
                .generate()
                .await
                .expect("generation succeeds");
            let second = Generator::new(12, 9, name, Some(8))
                .expect("known algorithm")
                .generate()
                .await
                .expect("generation succeeds");
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
