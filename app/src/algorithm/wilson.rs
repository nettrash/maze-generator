//! Wilson's algorithm.

use crate::{
    algorithm::{
        abstraction::AlgorithmAbstraction, carve_corridor, random_step, types::AlgorithmType,
    },
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
/// Like Aldous-Broder, its running time grows with the square of the maze's
/// longer side rather than with its number of cells; see
/// [`crate::generator::MAX_WALK_SIDE`].
#[derive(Debug, Clone)]
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
    /// The same as [`WilsonAlgorithm::new`]: every call picks a fresh random seed,
    /// so two defaults draw different mazes.
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
        if map.is_empty() || carve_corridor(&mut map) {
            return map;
        }
        let mut rng = self.rng();
        let w = width as usize;
        let h = height as usize;
        let cells = w * h;

        let mut in_maze = vec![false; cells];
        // The side each cell was last left by during the current walk.
        // Overwriting an entry when a cell is revisited is what erases the
        // loop. Entries left behind by earlier walks are never read: the
        // carving below starts at the walk's first cell and only ever follows
        // exits this walk wrote, so nothing has to be cleared between walks.
        // One byte per cell rather than the neighbour's coordinates, which
        // took 24.
        let mut exit = vec![Direction::North; cells];

        // Seed the maze with a single random cell.
        let start_index = rng.below(cells as u64) as usize;
        in_maze[start_index] = true;

        for index in 0..cells {
            if in_maze[index] {
                continue;
            }

            let mut current = (index % w, index / w);
            while !in_maze[current.1 * w + current.0] {
                let (nx, ny, direction) = random_step(current.0, current.1, w, h, &mut rng);
                exit[current.1 * w + current.0] = direction;
                current = (nx, ny);
            }

            // Carve the loop-erased path from the start cell to the maze.
            let mut cell = (index % w, index / w);
            while !in_maze[cell.1 * w + cell.0] {
                in_maze[cell.1 * w + cell.0] = true;
                let direction = exit[cell.1 * w + cell.0];
                map.carve(cell.0, cell.1, direction);
                cell = map
                    .neighbor(cell.0, cell.1, direction)
                    .expect("the walk only ever stepped inside the maze");
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
