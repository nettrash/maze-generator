use crate::maze::{
    algorithm::{
        abstraction::AlgorithmAbstraction, aldous_broder::AldousBroderAlgorithm,
        backtracking::BacktrackingAlgorithm, eller::EllerAlgorithm, kruskal::KruskalAlgorithm,
        prim::PrimAlgorithm, wilson::WilsonAlgorithm,
    },
    map::Map,
};
use std::io::Error;

pub struct Generator {
    pub width: u64,
    pub height: u64,
    pub algorithm: Box<dyn AlgorithmAbstraction>,
}

impl Generator {
    pub fn new(width: u64, height: u64, algorithm: &str) -> Self {
        let alg = match algorithm {
            "backtracking" => {
                Box::new(BacktrackingAlgorithm::new()) as Box<dyn AlgorithmAbstraction>
            }
            "prim" => Box::new(PrimAlgorithm::new()) as Box<dyn AlgorithmAbstraction>,
            "kruskal" => Box::new(KruskalAlgorithm::new()) as Box<dyn AlgorithmAbstraction>,
            "aldous_broder" => {
                Box::new(AldousBroderAlgorithm::new()) as Box<dyn AlgorithmAbstraction>
            }
            "wilson" => Box::new(WilsonAlgorithm::new()) as Box<dyn AlgorithmAbstraction>,
            "eller" => Box::new(EllerAlgorithm::new()) as Box<dyn AlgorithmAbstraction>,
            _ => {
                panic!("Unsupported algorithm: {}", algorithm);
            }
        };
        Generator {
            width,
            height,
            algorithm: alg,
        }
    }

    pub async fn generate(&self) -> Result<Map, Error> {
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
    use super::Generator;

    /// Every algorithm must return a perfect, solvable maze of the requested
    /// size, including the degenerate one-cell and single-row shapes.
    #[tokio::test]
    async fn all_algorithms_generate_perfect_mazes() {
        let algorithms = [
            "backtracking",
            "prim",
            "kruskal",
            "aldous_broder",
            "wilson",
            "eller",
        ];
        let sizes = [(1u64, 1u64), (1, 7), (6, 1), (9, 6), (12, 10)];

        for name in algorithms {
            for (width, height) in sizes {
                let generator = Generator::new(width, height, name);
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
        let cases = [
            ("backtracking", "backtracking"),
            ("prim", "prim"),
            ("kruskal", "kruskal"),
            ("aldous_broder", "aldous_broder"),
            ("wilson", "wilson"),
            ("eller", "eller"),
        ];
        for (name, expected) in cases {
            let generator = Generator::new(3, 3, name);
            assert_eq!(
                generator.algorithm.get_algorithm_type().as_string(),
                expected
            );
        }
    }

    #[test]
    #[should_panic(expected = "Unsupported algorithm")]
    fn rejects_an_unknown_algorithm() {
        Generator::new(3, 3, "not-an-algorithm");
    }
}
