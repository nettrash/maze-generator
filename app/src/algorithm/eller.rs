use crate::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::{Direction, Map},
    rng::{Rng, random_seed},
};

/// Eller's algorithm.
///
/// Rows are generated left to right, one at a time, keeping track of which
/// cells belong to the same set. Neighbouring cells in the same row are joined
/// when they are in different sets, and cells are dropped to the next row when
/// their set has more than one member left in the row. Only the final row is
/// forced to merge every remaining set. Because sets are never split, the
/// result is a spanning tree and the maze is perfect.
///
/// It only ever looks at one row, so it can generate very tall mazes cheaply.
pub struct EllerAlgorithm {
    seed: u64,
}

impl EllerAlgorithm {
    /// Creates the algorithm with a fresh random seed.
    pub fn new() -> Self {
        EllerAlgorithm {
            seed: random_seed(),
        }
    }

    /// Creates the algorithm with a fixed seed, which makes the mazes it
    /// produces reproducible.
    pub fn with_seed(seed: u64) -> Self {
        EllerAlgorithm { seed }
    }

    /// Creates the algorithm from an optional seed, falling back to a fresh
    /// random one when none is given.
    pub fn with_seed_or_random(seed: Option<u64>) -> Self {
        EllerAlgorithm {
            seed: seed.unwrap_or_else(random_seed),
        }
    }

    fn rng(&self) -> Rng {
        Rng::new(self.seed)
    }
}

impl Default for EllerAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for EllerAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::Eller
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

        // `set[x]` is the label of the set owning the cell at column `x` of the
        // current row. Merging relabels in place, so labels stay small.
        let mut set: Vec<usize> = (0..w).collect();
        let mut next_label = w;

        for y in 0..h {
            let last_row = y == h - 1;

            // Join with the neighbour on the right. On the last row every
            // boundary between two sets has to be opened, otherwise the maze
            // would end up disconnected.
            if w > 1 {
                for x in 0..w - 1 {
                    if set[x] == set[x + 1] {
                        continue;
                    }
                    if last_row || rng.coin() {
                        map.carve(x, y, Direction::East);
                        let (keep, gone) = (set[x], set[x + 1]);
                        for label in set.iter_mut() {
                            if *label == gone {
                                *label = keep;
                            }
                        }
                    }
                }
            }

            if last_row {
                break;
            }

            // The last cell of a set in this row must drop, otherwise the set
            // would vanish and its cells would be cut off from the rows below.
            let mut last_column = vec![usize::MAX; next_label];
            for x in 0..w {
                last_column[set[x]] = x;
            }

            let mut next_set = vec![0usize; w];
            for x in 0..w {
                let label = set[x];
                if last_column[label] == x || rng.coin() {
                    map.carve(x, y, Direction::South);
                    next_set[x] = label;
                } else {
                    // A fresh set begins in the next row.
                    next_set[x] = next_label;
                    next_label += 1;
                    last_column.push(usize::MAX);
                }
            }
            set = next_set;
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::EllerAlgorithm;
    use crate::algorithm::abstraction::AlgorithmAbstraction;

    #[test]
    fn generates_a_perfect_maze() {
        let algorithm = EllerAlgorithm::with_seed(11);
        let map = algorithm.generate_maze(14, 11);
        assert!(map.is_perfect());
        assert!(map.is_solvable());
    }

    #[test]
    fn handles_a_single_row() {
        let algorithm = EllerAlgorithm::with_seed(12);
        let map = algorithm.generate_maze(5, 1);
        assert!(map.is_perfect());
    }

    #[test]
    fn handles_a_single_column() {
        let algorithm = EllerAlgorithm::with_seed(13);
        let map = algorithm.generate_maze(1, 9);
        assert!(map.is_perfect());
    }

    #[test]
    fn many_seeds_all_produce_perfect_mazes() {
        for seed in 0..25 {
            let algorithm = EllerAlgorithm::with_seed(seed);
            let map = algorithm.generate_maze(9, 7);
            assert!(map.is_perfect(), "seed {seed} produced an imperfect maze");
        }
    }
}
