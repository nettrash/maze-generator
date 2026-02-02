use crate::maze::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::Map,
};

pub struct EllerAlgorithm {}

impl EllerAlgorithm {
    pub fn new() -> Self {
        EllerAlgorithm {}
    }
}

impl Default for EllerAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for EllerAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::Eller
    }

    fn generate_maze(&self, width: u64, height: u64) -> Map {
        //let map = Map::new(width, height);
        // Implement the eller maze generation algorithm here
        // For now, we return an empty map
        //map
        Map::new(width, height)
    }
}
