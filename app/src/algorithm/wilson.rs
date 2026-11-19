use crate::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::{Direction, Map},
    rng::{Rng, random_seed},
};

/// Wilson's algorithm using loop-erased random walks.
///
/// Cells are added to the maze one at a time. Each new cell performs a random
/// walk whose loops are erased as they form, continuing until it reaches a
/// cell already in the maze; the resulting simple path is then carved. This
/// produces mazes that are uniformly distributed over all perfect mazes.
///
/// Like Aldous-Broder it is slower than backtracking or Prim, so it is best
/// suited to small and medium grids.
pub struct WilsonAlgorithm {
    seed: u64,
}

impl WilsonAlgorithm {
    /// Creates the algorithm with a fresh random seed.
    pub fn new() -> Self {
        WilsonAlgorithm {
            seed: random_seed(),
        }
    }

    /// Creates the algorithm with a fixed seed, which makes the mazes it
    /// produces reproducible.
    pub fn with_seed(seed: u64) -> Self {
        WilsonAlgorithm { seed }
    }

    /// Creates the algorithm from an optional seed, falling back to a fresh
    /// random one when none is given.
    pub fn with_seed_or_random(seed: Option<u64>) -> Self {
        WilsonAlgorithm {
            seed: seed.unwrap_or_else(random_seed),
        }
    }

    fn rng(&self) -> Rng {
        Rng::new(self.seed)
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

    fn seed(&self) -> u64 {
        self.seed
    }

    fn generate_maze(&self, width: u64, height: u64) -> Map {
        let mut map = Map::new(width, height);
        if map.is_empty() {
            return map;
        }
        let mut rng = self.rng();
        let w = width as usize;
        let h = height as usize;
        let cells = w * h;

        let mut in_maze = vec![false; cells];
        // The successor recorded during the current walk. Overwriting an entry
        // when a cell is revisited is what erases the loop.
        let mut successor: Vec<Option<(usize, usize, Direction)>> = vec![None; cells];
        let mut touched: Vec<usize> = Vec::new();

        // Seed the maze with a single random cell.
        let start_index = rng.below(cells as u64) as usize;
        in_maze[start_index] = true;

        for index in 0..cells {
            if in_maze[index] {
                continue;
            }

            let mut current = (index % w, index / w);
            while !in_maze[current.1 * w + current.0] {
                let count = map.neighbor_count(current.0, current.1);
                let (nx, ny, direction) =
                    map.neighbor_at(current.0, current.1, rng.below(count as u64) as usize);
                let from = current.1 * w + current.0;
                if successor[from].is_none() {
                    touched.push(from);
                }
                successor[from] = Some((nx, ny, direction));
                current = (nx, ny);
            }

            // Carve the loop-erased path from the start cell to the maze.
            let mut cell = (index % w, index / w);
            in_maze[index] = true;
            while let Some((nx, ny, direction)) = successor[cell.1 * w + cell.0].take() {
                map.carve(cell.0, cell.1, direction);
                in_maze[ny * w + nx] = true;
                cell = (nx, ny);
            }

            for stale in touched.drain(..) {
                successor[stale] = None;
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::WilsonAlgorithm;
    use crate::algorithm::abstraction::AlgorithmAbstraction;

    #[test]
    fn generates_a_perfect_maze() {
        let algorithm = WilsonAlgorithm::with_seed(9);
        let map = algorithm.generate_maze(7, 7);
        assert!(map.is_perfect());
        assert!(map.is_solvable());
    }

    #[test]
    fn handles_a_single_cell() {
        let algorithm = WilsonAlgorithm::with_seed(10);
        let map = algorithm.generate_maze(1, 1);
        assert!(map.is_perfect());
    }
}
