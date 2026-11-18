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
    pub fn new(
        width: u64,
        height: u64,
        algorithm: &str,
        seed: Option<u64>,
    ) -> Result<Self, MazeError> {
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

    pub async fn generate(&self) -> Result<Map, MazeError> {
        println!(
            "Generating a {}x{} maze using the '{}' algorithm (seed {})...",
            self.width,
            self.height,
            self.algorithm.get_algorithm_type().as_string(),
            self.seed()
        );
        Ok(self.build())
    }

    /// Runs the algorithm and applies the requested variants.
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
    use super::{ALGORITHMS, Generator};

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
