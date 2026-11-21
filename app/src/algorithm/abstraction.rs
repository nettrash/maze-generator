//! The interface every generation algorithm implements.

use crate::{algorithm::types::AlgorithmType, map::Map};

/// Defines the abstraction for maze generation algorithms.
///
/// `Send + Sync` so that a [`crate::Generator`], which holds its algorithm
/// behind a box, can be moved to or shared with another thread. Every
/// algorithm here is a seed and nothing else, so each of them qualifies.
pub trait AlgorithmAbstraction: Send + Sync {
    /// Returns the type of the algorithm.
    fn get_algorithm_type(&self) -> AlgorithmType;
    /// The seed the algorithm draws its randomness from.
    ///
    /// Reporting it lets a run be replayed exactly by rebuilding the algorithm
    /// with `with_seed`.
    fn seed(&self) -> u64;
    /// Generates a maze with the specified width and height.
    ///
    /// Nothing here limits the size; [`crate::Generator::new`] is where a
    /// request is checked against [`crate::generator::MAX_CELLS`]. A zero
    /// dimension gives an empty map.
    ///
    /// # Panics
    ///
    /// Panics, or aborts on a failed allocation, when the maze is too large
    /// to allocate, as [`Map::new`] does.
    fn generate_maze(&self, width: u64, height: u64) -> Map;
}
