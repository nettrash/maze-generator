use crate::maze::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::Map,
};

pub struct AldousBroderAlgorithm {}

impl AldousBroderAlgorithm {
    pub fn new() -> Self {
        AldousBroderAlgorithm {}
    }
}

impl Default for AldousBroderAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for AldousBroderAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::AldousBroder
    }

    fn generate_maze(&self, width: u64, height: u64) -> Map {
        //let map = Map::new(width, height);
        // Implement the aldous broder maze generation algorithm here
        // For now, we return an empty map
        //map
        Map::new(width, height)
    }
}
