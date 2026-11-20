//! Randomised Kruskal's algorithm.

use crate::{
    algorithm::{Index, abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::{Direction, Map},
    rng::{Rng, random_seed},
};

/// Union-find over cell indices, used by Kruskal's algorithm to tell whether
/// two cells are already connected.
struct DisjointSet<I> {
    parent: Vec<I>,
    rank: Vec<u8>,
}

impl<I: Index> DisjointSet<I> {
    fn new(size: usize) -> Self {
        DisjointSet {
            parent: (0..size).map(I::from_usize).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, item: usize) -> usize {
        let mut item = item;
        while self.parent[item].to_usize() != item {
            // Path halving keeps the trees shallow without recursion.
            let grandparent = self.parent[self.parent[item].to_usize()];
            self.parent[item] = grandparent;
            item = grandparent.to_usize();
        }
        item
    }

    /// Joins two sets, returning whether they were previously separate.
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (root_a, root_b) = (self.find(a), self.find(b));
        if root_a == root_b {
            return false;
        }
        match self.rank[root_a].cmp(&self.rank[root_b]) {
            std::cmp::Ordering::Less => self.parent[root_a] = I::from_usize(root_b),
            std::cmp::Ordering::Greater => self.parent[root_b] = I::from_usize(root_a),
            std::cmp::Ordering::Equal => {
                self.parent[root_b] = I::from_usize(root_a);
                self.rank[root_a] += 1;
            }
        }
        true
    }
}

/// Randomised Kruskal's algorithm.
///
/// Every interior wall is put in a list and shuffled; walls are removed in that
/// random order whenever they join two regions that are not connected yet.
/// Skipping walls that would close a loop leaves exactly a spanning tree, so
/// the maze is perfect.
#[derive(Debug, Clone)]
pub struct KruskalAlgorithm {
    seed: u64,
}

impl KruskalAlgorithm {
    /// Creates the algorithm with a fresh random seed.
    pub fn new() -> Self {
        KruskalAlgorithm {
            seed: random_seed(),
        }
    }

    /// Creates the algorithm with a fixed seed, which makes the mazes it
    /// produces reproducible.
    pub fn with_seed(seed: u64) -> Self {
        KruskalAlgorithm { seed }
    }

    /// Creates the algorithm from an optional seed, falling back to a fresh
    /// random one when none is given.
    pub fn with_seed_or_random(seed: Option<u64>) -> Self {
        KruskalAlgorithm {
            seed: seed.unwrap_or_else(random_seed),
        }
    }

    fn rng(&self) -> Rng {
        Rng::new(self.seed)
    }
}

impl Default for KruskalAlgorithm {
    /// The same as [`KruskalAlgorithm::new`]: every call picks a fresh random seed,
    /// so two defaults draw different mazes.
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for KruskalAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::Kruskal
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
        // Wall numbers run up to twice the cell count, see `carve_walls`. The
        // list of walls alone used to be 1.2 GB of `(usize, usize,
        // Direction)` tuples at the largest size the generator accepts.
        if u32::holds(2 * w * h) {
            carve_walls::<u32>(&mut map, w, h, &mut rng);
        } else {
            carve_walls::<usize>(&mut map, w, h, &mut rng);
        }
        map
    }
}

/// Removes the walls of a `w` by `h` maze in a random order, skipping each
/// one that would close a loop.
///
/// Each wall is stored once, as `2 * cell + side` for the cell above or to the
/// left of it, with `side` 0 for its east wall and 1 for its south wall, so no
/// wall is considered twice and an entry is a single number. The list is
/// built in reading order and shuffled exactly as the tuples it replaces were,
/// so a seed draws the same maze it always has.
fn carve_walls<I: Index>(map: &mut Map, w: usize, h: usize, rng: &mut Rng) {
    let mut walls: Vec<I> = Vec::with_capacity(2 * w * h - w - h);
    for y in 0..h {
        for x in 0..w {
            let cell = y * w + x;
            if x + 1 < w {
                walls.push(I::from_usize(2 * cell));
            }
            if y + 1 < h {
                walls.push(I::from_usize(2 * cell + 1));
            }
        }
    }
    rng.shuffle(&mut walls);

    let mut sets = DisjointSet::<I>::new(w * h);
    for wall in walls {
        let wall = wall.to_usize();
        let cell = wall / 2;
        let (direction, neighbor) = if wall % 2 == 0 {
            (Direction::East, cell + 1)
        } else {
            (Direction::South, cell + w)
        };
        if sets.union(cell, neighbor) {
            map.carve(cell % w, cell / w, direction);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::KruskalAlgorithm;
    use crate::algorithm::abstraction::AlgorithmAbstraction;

    #[test]
    fn generates_a_perfect_maze() {
        let algorithm = KruskalAlgorithm::with_seed(5);
        let map = algorithm.generate_maze(15, 9);
        assert!(map.is_perfect());
        assert!(map.is_solvable());
    }

    #[test]
    fn handles_a_single_cell() {
        let algorithm = KruskalAlgorithm::with_seed(6);
        let map = algorithm.generate_maze(1, 1);
        assert!(map.is_perfect());
    }
}
