use crate::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::Map,
    rng::{Rng, random_seed},
};

/// Aldous-Broder algorithm.
///
/// A uniform random walk visits every cell; a wall is removed only when the
/// walk steps into a cell that has not been visited before. Connecting each
/// new cell to exactly one earlier cell yields a spanning tree, so the maze is
/// perfect and is sampled uniformly at random.
///
/// The random walk is slow to cover large grids, so this is best suited to
/// small mazes.
pub struct AldousBroderAlgorithm {
    seed: u64,
}

impl AldousBroderAlgorithm {
    /// Creates the algorithm with a fresh random seed.
    pub fn new() -> Self {
        AldousBroderAlgorithm {
            seed: random_seed(),
        }
    }

    /// Creates the algorithm with a fixed seed, which makes the mazes it
    /// produces reproducible.
    pub fn with_seed(seed: u64) -> Self {
        AldousBroderAlgorithm { seed }
    }

    /// Creates the algorithm from an optional seed, falling back to a fresh
    /// random one when none is given.
    pub fn with_seed_or_random(seed: Option<u64>) -> Self {
        AldousBroderAlgorithm {
            seed: seed.unwrap_or_else(random_seed),
        }
    }

    fn rng(&self) -> Rng {
        Rng::new(self.seed)
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

    fn seed(&self) -> u64 {
        self.seed
    }

    fn generate_maze(&self, width: u64, height: u64) -> Map {
        let mut map = Map::new(width, height);
        if map.is_empty() {
            return map;
        }
        let mut rng = self.rng();
        let cells = (width * height) as usize;
        let mut visited = vec![false; cells];
        let mut remaining = cells;

        let mut current = (rng.below(width) as usize, rng.below(height) as usize);
        visited[current.1 * width as usize + current.0] = true;
        remaining -= 1;

        while remaining > 0 {
            let neighbors = map.neighbors(current.0, current.1);
            let (nx, ny, direction) = neighbors[rng.below(neighbors.len() as u64) as usize];
            let index = ny * width as usize + nx;
            if !visited[index] {
                // Only the step that first reaches a cell opens a wall, which
                // is what keeps the result free of loops.
                map.carve(current.0, current.1, direction);
                visited[index] = true;
                remaining -= 1;
            }
            current = (nx, ny);
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::AldousBroderAlgorithm;
    use crate::algorithm::abstraction::AlgorithmAbstraction;

    #[test]
    fn generates_a_perfect_maze() {
        let algorithm = AldousBroderAlgorithm::with_seed(7);
        let map = algorithm.generate_maze(6, 5);
        assert!(map.is_perfect());
        assert!(map.is_solvable());
    }

    #[test]
    fn handles_a_single_cell() {
        let algorithm = AldousBroderAlgorithm::with_seed(8);
        let map = algorithm.generate_maze(1, 1);
        assert!(map.is_perfect());
    }
}
