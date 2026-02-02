use crate::maze::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::Map,
};

pub struct KruskalAlgorithm {}

impl KruskalAlgorithm {
    pub fn new() -> Self {
        KruskalAlgorithm {}
    }
}

impl Default for KruskalAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for KruskalAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::Kruskal
    }

    fn generate_maze(&self, width: u64, height: u64) -> Map {
        //let map = Map::new(width, height);
        // Implement the kruskal maze generation algorithm here
        // For now, we return an empty map
        //map
        Map::new(width, height)
    }
}
