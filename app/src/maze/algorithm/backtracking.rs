use crate::maze::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::Map,
};

pub struct BacktrackingAlgorithm {}

impl BacktrackingAlgorithm {
    pub fn new() -> Self {
        BacktrackingAlgorithm {}
    }
}

impl Default for BacktrackingAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for BacktrackingAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::Backtracking
    }

    fn generate_maze(&self, width: u64, height: u64) -> Map {
        //let map = Map::new(width, height);
        // Implement the backtracking maze generation algorithm here
        // For now, we return an empty map
        //map
        Map::new(width, height)
    }
}
