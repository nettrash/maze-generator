use crate::{
    algorithm::{
        abstraction::AlgorithmAbstraction, aldous_broder::AldousBroderAlgorithm,
        backtracking::BacktrackingAlgorithm, eller::EllerAlgorithm, kruskal::KruskalAlgorithm,
        prim::PrimAlgorithm, wilson::WilsonAlgorithm,
    },
    error::MazeError,
    map::Map,
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

pub struct Generator {
    pub width: u64,
    pub height: u64,
    pub algorithm: Box<dyn AlgorithmAbstraction>,
}

impl Generator {
    /// Builds a generator for the named algorithm.
    ///
    /// Returns [`MazeError::UnsupportedAlgorithm`] when `algorithm` is not one
    /// of [`ALGORITHMS`], so a mistyped name is reported to the user instead of
    /// aborting the process.
    pub fn new(width: u64, height: u64, algorithm: &str) -> Result<Self, MazeError> {
        let alg: Box<dyn AlgorithmAbstraction> = match algorithm {
            "backtracking" => Box::new(BacktrackingAlgorithm::new()),
            "prim" => Box::new(PrimAlgorithm::new()),
            "kruskal" => Box::new(KruskalAlgorithm::new()),
            "aldous_broder" => Box::new(AldousBroderAlgorithm::new()),
            "wilson" => Box::new(WilsonAlgorithm::new()),
            "eller" => Box::new(EllerAlgorithm::new()),
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
        })
    }

    pub async fn generate(&self) -> Result<Map, MazeError> {
        println!(
            "Generating a {}x{} maze using the '{}' algorithm...",
            self.width,
            self.height,
            self.algorithm.get_algorithm_type().as_string()
        );
        let map = self.algorithm.generate_maze(self.width, self.height);
        Ok(map)
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
                let generator = Generator::new(width, height, name).expect("known algorithm");
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
            let generator = Generator::new(3, 3, name).expect("known algorithm");
            assert_eq!(generator.algorithm.get_algorithm_type().as_string(), name);
        }
    }

    #[test]
    fn unknown_algorithm_is_an_error_not_a_panic() {
        let result = Generator::new(3, 3, "not-an-algorithm");
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
                Generator::new(3, 3, bad).is_err(),
                "{bad:?} should not be accepted"
            );
        }
    }
}
