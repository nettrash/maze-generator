use crate::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::{Direction, Map},
    rng::Rng,
};

/// Randomised Prim's algorithm.
///
/// A frontier of walls between the growing maze and the untouched cells is
/// kept; a random frontier wall is removed each step and the walls of the newly
/// reached cell are added to the frontier. Every carve connects a cell that was
/// not in the maze before, so the removed walls form a spanning tree and the
/// maze is perfect.
pub struct PrimAlgorithm {
    seed: Option<u64>,
}

impl PrimAlgorithm {
    pub fn new() -> Self {
        PrimAlgorithm { seed: None }
    }

    /// Creates the algorithm with a fixed seed so its output is reproducible.
    pub fn with_seed(seed: u64) -> Self {
        PrimAlgorithm { seed: Some(seed) }
    }

    fn rng(&self) -> Rng {
        match self.seed {
            Some(seed) => Rng::new(seed),
            None => Rng::from_entropy(),
        }
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
        let mut map = Map::new(width, height);
        if map.is_empty() {
            return map;
        }
        let mut rng = self.rng();
        let w = width as usize;
        let h = height as usize;
        let mut in_maze = vec![vec![false; w]; h];

        // Frontier entries are the `(neighbour, direction)` triples returned by
        // `Map::neighbors`, so the wall to open is on the neighbour's side
        // facing back towards the maze.
        let mut frontier: Vec<(usize, usize, Direction)> = Vec::new();
        let start = (rng.below(width) as usize, rng.below(height) as usize);
        in_maze[start.1][start.0] = true;
        frontier.extend(map.neighbors(start.0, start.1));

        while !frontier.is_empty() {
            let index = rng.below(frontier.len() as u64) as usize;
            let (nx, ny, direction) = frontier.swap_remove(index);
            if in_maze[ny][nx] {
                continue;
            }
            map.carve(nx, ny, direction.opposite());
            in_maze[ny][nx] = true;
            frontier.extend(map.neighbors(nx, ny));
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::PrimAlgorithm;
    use crate::algorithm::abstraction::AlgorithmAbstraction;

    #[test]
    fn generates_a_perfect_maze() {
        let algorithm = PrimAlgorithm::with_seed(3);
        let map = algorithm.generate_maze(10, 10);
        assert!(map.is_perfect());
        assert!(map.is_solvable());
    }

    #[test]
    fn handles_a_single_row() {
        let algorithm = PrimAlgorithm::with_seed(4);
        let map = algorithm.generate_maze(6, 1);
        assert!(map.is_perfect());
    }
}
