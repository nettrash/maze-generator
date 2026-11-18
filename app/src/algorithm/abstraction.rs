use crate::{algorithm::types::AlgorithmType, map::Map};

/// Defines the abstraction for maze generation algorithms.
pub trait AlgorithmAbstraction {
    /// Returns the type of the algorithm.
    fn get_algorithm_type(&self) -> AlgorithmType;
    /// The seed the algorithm draws its randomness from.
    ///
    /// Reporting it lets a run be replayed exactly by rebuilding the algorithm
    /// with `with_seed`.
    fn seed(&self) -> u64;
    /// Generates a maze with the specified width and height.
    fn generate_maze(&self, width: u64, height: u64) -> Map;
}
