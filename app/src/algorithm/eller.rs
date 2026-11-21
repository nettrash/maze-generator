//! Eller's algorithm.

use crate::{
    algorithm::{Index, abstraction::AlgorithmAbstraction, types::AlgorithmType},
    map::{Direction, Map},
    rng::{Rng, random_seed},
};

/// Eller's algorithm.
///
/// Rows are carved top to bottom, keeping track of which cells of the
/// current row are already connected, through the rows above, as sets. Within
/// a row, each pair of neighbouring cells in different sets is joined on a
/// coin flip, and always on the last row, which joins every remaining set.
/// Every cell then opens downwards on a coin flip, and a set that opened none
/// opens one cell chosen uniformly among its members, so that no set is cut
/// off from the rows below. Because sets are only ever merged, never split,
/// the result is a spanning tree and the maze is perfect.
///
/// It only ever looks at one row, so it can generate very tall mazes cheaply:
/// its time and memory grow with the number of cells and with the width
/// respectively.
#[derive(Debug, Clone)]
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
    /// The same as [`EllerAlgorithm::new`]: every call picks a fresh random seed,
    /// so two defaults draw different mazes.
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
        let (w, h) = (width as usize, height as usize);
        if u32::holds(w) {
            carve_rows::<u32>(&mut map, w, h, &mut rng);
        } else {
            carve_rows::<usize>(&mut map, w, h, &mut rng);
        }
        map
    }
}

/// Carves the maze row by row; see [`EllerAlgorithm`].
fn carve_rows<I: Index>(map: &mut Map, w: usize, h: usize, rng: &mut Rng) {
    // The sets of the current row form a union-find forest over its columns:
    // two cells share a set exactly when their columns share a root. Every
    // set still has a cell in the row being carved, so one of its columns can
    // always stand for it and the forest needs no more than `w` entries,
    // however many rows came before. Labels used to be fresh numbers that
    // grew with every row, and both the relabelling of a whole row on every
    // join and a table sized by every label ever made paid for that growth:
    // a 5000x5000 maze took 16 s and 4.6 GB.
    let mut parent: Vec<I> = (0..w).map(I::from_usize).collect();
    let mut below: Option<Below<I>> = None;

    for y in 0..h {
        let last_row = y == h - 1;

        // Join with the neighbour on the right. On the last row every
        // boundary between two sets has to be opened, otherwise the maze
        // would end up disconnected.
        for x in 0..w - 1 {
            let (left, right) = (find(&mut parent, x), find(&mut parent, x + 1));
            if left == right {
                continue;
            }
            if last_row || rng.coin() {
                map.carve(x, y, Direction::East);
                parent[right] = I::from_usize(left);
            }
        }

        if last_row {
            break;
        }
        below
            .get_or_insert_with(|| Below::new(w))
            .open_downwards(map, y, &mut parent, rng);
    }
}

/// The bookkeeping for opening a row downwards, kept between rows so that it
/// is allocated once. A maze one row tall never opens downwards and never
/// needs it.
struct Below<I> {
    /// The root of each column's set.
    root: Vec<I>,
    /// Whether each column opens downwards.
    drops: Vec<bool>,
    /// The forest of the next row while it is being built.
    next_parent: Vec<I>,
    /// Per set, indexed by its root: how many cells it has in the row ...
    size: Vec<I>,
    /// ... whether any of them opened downwards on its own ...
    dropped: Vec<bool>,
    /// ... which of its cells, counted in column order, opens if none did ...
    pick: Vec<I>,
    /// ... how many of its cells have been passed so far ...
    seen: Vec<I>,
    /// ... and the first of its cells to open, which stands for the set in
    /// the row below.
    first: Vec<I>,
}

impl<I: Index> Below<I> {
    fn new(w: usize) -> Self {
        Below {
            root: vec![I::NONE; w],
            drops: vec![false; w],
            next_parent: vec![I::NONE; w],
            size: vec![I::from_usize(0); w],
            dropped: vec![false; w],
            pick: vec![I::NONE; w],
            seen: vec![I::from_usize(0); w],
            first: vec![I::NONE; w],
        }
    }

    /// Opens row `y` downwards and turns `parent` into the forest of the next
    /// row.
    ///
    /// Every cell opens on a coin flip. A set that opened none would vanish,
    /// cutting its cells off from the rows below, so it opens one of its
    /// cells chosen uniformly. This used to force the last cell of every set
    /// open instead, and since the cell in the last column is always the last
    /// of its set, the last column of every Eller maze was one straight
    /// corridor down to the exit.
    fn open_downwards(&mut self, map: &mut Map, y: usize, parent: &mut [I], rng: &mut Rng) {
        let w = parent.len();
        for x in 0..w {
            let root = find(parent, x);
            self.root[x] = I::from_usize(root);
            self.drops[x] = rng.coin();
            self.dropped[root] |= self.drops[x];
            self.size[root] = I::from_usize(self.size[root].to_usize() + 1);
        }

        // Each set that opened nothing chooses its cell, in the order the
        // sets first appear in the row.
        for x in 0..w {
            let root = self.root[x].to_usize();
            if !self.dropped[root] && self.pick[root] == I::NONE {
                let size = self.size[root].to_usize() as u64;
                self.pick[root] = I::from_usize(rng.below(size) as usize);
            }
        }

        for x in 0..w {
            let root = self.root[x].to_usize();
            let place = self.seen[root];
            self.seen[root] = I::from_usize(place.to_usize() + 1);
            if self.drops[x] || (!self.dropped[root] && self.pick[root] == place) {
                map.carve(x, y, Direction::South);
                if self.first[root] == I::NONE {
                    self.first[root] = I::from_usize(x);
                }
                self.next_parent[x] = self.first[root];
            } else {
                // A fresh set begins in the next row.
                self.next_parent[x] = I::from_usize(x);
            }
        }

        for x in 0..w {
            let root = self.root[x].to_usize();
            self.size[root] = I::from_usize(0);
            self.dropped[root] = false;
            self.pick[root] = I::NONE;
            self.seen[root] = I::from_usize(0);
            self.first[root] = I::NONE;
        }
        parent.copy_from_slice(&self.next_parent);
    }
}

/// The root of the set holding `column`, halving the path on the way up so
/// the next lookup is shorter.
fn find<I: Index>(parent: &mut [I], column: usize) -> usize {
    let mut column = column;
    while parent[column].to_usize() != column {
        let grandparent = parent[parent[column].to_usize()];
        parent[column] = grandparent;
        column = grandparent.to_usize();
    }
    column
}

#[cfg(test)]
mod tests {
    use super::EllerAlgorithm;
    use crate::algorithm::abstraction::AlgorithmAbstraction;
    use crate::map::Direction;
    use std::collections::HashSet;

    /// Forcing the last cell of every set open made the last column of every
    /// maze one straight corridor down to the exit: 200 of 200 seeds.
    #[test]
    fn the_last_column_is_not_always_one_corridor() {
        let straight = (0..200)
            .filter(|&seed| {
                let map = EllerAlgorithm::with_seed(seed).generate_maze(12, 12);
                (0..11).all(|y| !map.has_wall(11, y, Direction::South))
            })
            .count();
        assert!(
            straight < 40,
            "the last column was one corridor in {straight} of 200 mazes"
        );
    }

    /// A 2x2 grid has four perfect mazes, and the forced opening made one of
    /// them, the maze without a passage between (1, 0) and (1, 1), impossible.
    #[test]
    fn every_two_by_two_maze_can_be_drawn() {
        let drawn: HashSet<String> = (0..400)
            .map(|seed| {
                EllerAlgorithm::with_seed(seed)
                    .generate_maze(2, 2)
                    .to_ascii()
            })
            .collect();
        assert_eq!(drawn.len(), 4, "{drawn:#?}");
    }

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
