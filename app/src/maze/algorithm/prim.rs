use crate::maze::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::Map,
};

pub struct PrimAlgorithm {}

impl PrimAlgorithm {
    pub fn new() -> Self {
        PrimAlgorithm {}
    }
}

impl Default for PrimAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for PrimAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::Prim
    }

    fn generate_maze(&self, width: u64, height: u64) -> Map {
        //let map = Map::new(width, height);
        // Implement the prim maze generation algorithm here
        // For now, we return an empty map
        //map
        Map::new(width, height)
    }
}
