use crate::{
    algorithm::{abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::{Direction, Map},
    rng::Rng,
};

/// Union-find over cell indices, used by Kruskal's algorithm to tell whether
/// two cells are already connected.
struct DisjointSet {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl DisjointSet {
    fn new(size: usize) -> Self {
        DisjointSet {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, mut item: usize) -> usize {
        while self.parent[item] != item {
            // Path halving keeps the trees shallow without recursion.
            let grandparent = self.parent[self.parent[item]];
            self.parent[item] = grandparent;
            item = grandparent;
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
            std::cmp::Ordering::Less => self.parent[root_a] = root_b,
            std::cmp::Ordering::Greater => self.parent[root_b] = root_a,
            std::cmp::Ordering::Equal => {
                self.parent[root_b] = root_a;
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
pub struct KruskalAlgorithm {
    seed: Option<u64>,
}

impl KruskalAlgorithm {
    pub fn new() -> Self {
        KruskalAlgorithm { seed: None }
    }

    /// Creates the algorithm with a fixed seed so its output is reproducible.
    pub fn with_seed(seed: u64) -> Self {
        KruskalAlgorithm { seed: Some(seed) }
    }

    fn rng(&self) -> Rng {
        match self.seed {
            Some(seed) => Rng::new(seed),
            None => Rng::from_entropy(),
        }
    }
}

impl Default for KruskalAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmAbstraction for KruskalAlgorithm {
    fn get_algorithm_type(&self) -> AlgorithmType {
        AlgorithmType::Kruskal
    }

    fn generate_maze(&self, width: u64, height: u64) -> Map {
        let mut map = Map::new(width, height);
        if map.is_empty() {
            return map;
        }
        let mut rng = self.rng();
        let w = width as usize;
        let h = height as usize;

        // Each wall is stored once, from the cell that lies above or to the
        // left of it, so no wall is considered twice.
        let mut walls: Vec<(usize, usize, Direction)> = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if x + 1 < w {
                    walls.push((x, y, Direction::East));
                }
                if y + 1 < h {
                    walls.push((x, y, Direction::South));
                }
            }
        }
        rng.shuffle(&mut walls);

        let mut sets = DisjointSet::new(w * h);
        for (x, y, direction) in walls {
            let (nx, ny) = match direction {
                Direction::East => (x + 1, y),
                Direction::South => (x, y + 1),
                _ => continue,
            };
            if sets.union(y * w + x, ny * w + nx) {
                map.carve(x, y, direction);
            }
        }
        map
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
