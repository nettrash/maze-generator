use crate::maze::{algorithm::types::AlgorithmType, map::Map};

/// Defines the abstraction for maze generation algorithms.
pub trait AlgorithmAbstraction {
    /// Returns the type of the algorithm.
    fn get_algorithm_type(&self) -> AlgorithmType;
    /// Generates a maze with the specified width and height.
    fn generate_maze(&self, width: u64, height: u64) -> Map;
}
