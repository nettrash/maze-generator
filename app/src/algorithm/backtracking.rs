//! The recursive backtracker.

use crate::{
    algorithm::{Index, abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::{Direction, Map},
    rng::{Rng, random_seed},
};

/// Recursive backtracker: a depth-first walk that carves as it advances and
/// backtracks whenever it reaches a dead end.
///
/// It visits every cell exactly once, so the walls it removes form a spanning
/// tree and the result is always a perfect maze.
#[derive(Debug, Clone)]
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
    /// The same as [`BacktrackingAlgorithm::new`]: every call picks a fresh random seed,
    /// so two defaults draw different mazes.
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
        let (w, h) = (width as usize, height as usize);
        if u32::holds(w * h) {
            walk::<u32>(&mut map, w, h, &mut rng);
        } else {
            walk::<usize>(&mut map, w, h, &mut rng);
        }
        map
    }
}

/// The depth-first walk itself, with the path back to the start kept as a
/// stack of flat cell numbers.
///
/// The stack can hold most of the maze at once, every cell of a long
/// corridor, and a pair of `usize` coordinates per entry made it 400 MB on a
/// 1x25000000 maze; a `u32` per entry is a quarter of that.
fn walk<I: Index>(map: &mut Map, w: usize, h: usize, rng: &mut Rng) {
    let mut stack: Vec<I> = vec![I::from_usize(0)];
    let mut visited = vec![false; w * h];
    visited[0] = true;

    while let Some(&top) = stack.last() {
        let index = top.to_usize();
        let (x, y) = (index % w, index / w);
        // At most four neighbours exist, so the unvisited ones fit in a
        // fixed array; allocating a Vec here would run once per step.
        let mut unvisited: [(usize, usize, Direction); 4] = [(0, 0, Direction::North); 4];
        let mut count = 0;
        for (nx, ny, direction) in map.each_neighbor(x, y) {
            if !visited[ny * w + nx] {
                unvisited[count] = (nx, ny, direction);
                count += 1;
            }
        }

        if count == 0 {
            stack.pop();
            continue;
        }
        let (nx, ny, direction) = unvisited[rng.below(count as u64) as usize];
        map.carve(x, y, direction);
        visited[ny * w + nx] = true;
        stack.push(I::from_usize(ny * w + nx));
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
