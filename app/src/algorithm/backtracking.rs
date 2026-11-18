use crate::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::Map,
    rng::{Rng, random_seed},
};

/// Recursive backtracker: a depth-first walk that carves as it advances and
/// backtracks whenever it reaches a dead end.
///
/// It visits every cell exactly once, so the walls it removes form a spanning
/// tree and the result is always a perfect maze.
pub struct BacktrackingAlgorithm {
    seed: u64,
}

impl BacktrackingAlgorithm {
    /// Creates the algorithm with a fresh random seed.
    pub fn new() -> Self {
        BacktrackingAlgorithm {
            seed: random_seed(),
        }
    }

    /// Creates the algorithm with a fixed seed, which makes the mazes it
    /// produces reproducible.
    pub fn with_seed(seed: u64) -> Self {
        BacktrackingAlgorithm { seed }
    }

    /// Creates the algorithm from an optional seed, falling back to a fresh
    /// random one when none is given.
    pub fn with_seed_or_random(seed: Option<u64>) -> Self {
        BacktrackingAlgorithm {
            seed: seed.unwrap_or_else(random_seed),
        }
    }

    fn rng(&self) -> Rng {
        Rng::new(self.seed)
    }
}

impl Default for BacktrackingAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for BacktrackingAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::Backtracking
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
        let mut stack = vec![(0usize, 0usize)];
        let mut visited = vec![vec![false; width as usize]; height as usize];
        visited[0][0] = true;

        while let Some((x, y)) = stack.last().copied() {
            let unvisited: Vec<_> = map
                .neighbors(x, y)
                .into_iter()
                .filter(|&(nx, ny, _)| !visited[ny][nx])
                .collect();

            if unvisited.is_empty() {
                stack.pop();
                continue;
            }
            let (nx, ny, direction) = unvisited[rng.below(unvisited.len() as u64) as usize];
            map.carve(x, y, direction);
            visited[ny][nx] = true;
            stack.push((nx, ny));
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::BacktrackingAlgorithm;
    use crate::algorithm::abstraction::AlgorithmAbstraction;

    #[test]
    fn generates_a_perfect_maze() {
        let algorithm = BacktrackingAlgorithm::with_seed(1);
        let map = algorithm.generate_maze(12, 8);
        assert!(map.is_perfect());
        assert!(map.is_solvable());
    }

    #[test]
    fn handles_a_single_cell() {
        let algorithm = BacktrackingAlgorithm::with_seed(2);
        let map = algorithm.generate_maze(1, 1);
        assert!(map.is_perfect());
    }
}
