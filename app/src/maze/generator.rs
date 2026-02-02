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
        // Maze generation logic would go here
        let map = self.algorithm.generate_maze(self.width, self.height);
        Ok(map)
    }
}
