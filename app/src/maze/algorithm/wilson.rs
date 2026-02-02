use crate::maze::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::Map,
};

pub struct WilsonAlgorithm {}

impl WilsonAlgorithm {
    pub fn new() -> Self {
        WilsonAlgorithm {}
    }
}

impl Default for WilsonAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for WilsonAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::Wilson
    }

    fn generate_maze(&self, width: u64, height: u64) -> Map {
        //let map = Map::new(width, height);
        // Implement the wilson maze generation algorithm here
        // For now, we return an empty map
        //map
        Map::new(width, height)
    }
}
