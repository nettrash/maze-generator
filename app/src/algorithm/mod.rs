//! The generation algorithms, one module each, behind
//! [`abstraction::AlgorithmAbstraction`].
//!
//! Every algorithm produces a *perfect* maze, a spanning tree of the grid's
//! cells, and draws all of its choices from a seed, so a seed reproduces its
//! maze. They differ in the texture of the mazes they draw and in what they
//! cost; the README compares them.

use crate::map::{Direction, Map};
use crate::rng::Rng;

pub mod abstraction;
pub mod aldous_broder;
pub mod backtracking;
pub mod eller;
pub mod kruskal;
pub mod prim;
pub mod types;
pub mod wilson;

/// A cell, column or wall number, stored as compactly as the maze allows.
///
/// The algorithms keep tables with an entry per cell, column or wall, so the
/// width of an entry sets the memory they need. A `u32` holds every number a
/// maze from [`crate::Generator`] needs, at half the size of a `usize`, and
/// `usize` covers a larger maze built through the library.
pub(crate) trait Index: Copy + Eq {
    /// A value no real number takes, for marking an empty slot.
    const NONE: Self;

    /// The number as this type; it must be one [`Index::holds`] allowed.
    fn from_usize(value: usize) -> Self;

    /// The number back as a `usize`.
    fn to_usize(self) -> usize;

    /// Whether every number below `count` fits, with [`Index::NONE`] to spare.
    fn holds(count: usize) -> bool;
}

impl Index for u32 {
    const NONE: Self = u32::MAX;

    fn from_usize(value: usize) -> Self {
        value as u32
    }

    fn to_usize(self) -> usize {
        self as usize
    }

    fn holds(count: usize) -> bool {
        count < u32::MAX as usize
    }
}

impl Index for usize {
    const NONE: Self = usize::MAX;

    fn from_usize(value: usize) -> Self {
        value
    }

    fn to_usize(self) -> usize {
        self
    }

    fn holds(count: usize) -> bool {
        count < usize::MAX
    }
}

/// One step of a random walk from `(x, y)` in a `w` by `h` grid: a neighbour
/// chosen uniformly among those inside the grid.
///
/// It is exactly the neighbour `map.neighbor_at(x, y,
/// rng.below(map.neighbor_count(x, y)))` picks, from the same single draw, so
/// a walk built on it draws the same maze. The walks of Aldous-Broder and
/// Wilson take billions of steps on a large maze, and going through
/// `neighbor_at` branches on the direction drawn, which is random, so the
/// processor guessed the branch wrong half the time. Here the choice is a
/// table lookup and the step an addition.
pub(crate) fn random_step(
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    rng: &mut Rng,
) -> (usize, usize, Direction) {
    let inside = usize::from(y > 0)
        | usize::from(x + 1 < w) << 1
        | usize::from(y + 1 < h) << 2
        | usize::from(x > 0) << 3;
    let (sides, count) = &STEPS[inside];
    let direction = sides[rng.below_small(*count)];
    let (dx, dy) = OFFSETS[direction as usize];
    (
        x.wrapping_add_signed(dx),
        y.wrapping_add_signed(dy),
        direction,
    )
}

/// For each set of sides that lead inside the grid, as a mask with north 1,
/// east 2, south 4 and west 8: those sides in [`Direction::ALL`] order, which
/// is the order `neighbor_at` counts them in, and how many there are.
const STEPS: [([Direction; 4], usize); 16] = {
    let mut table = [([Direction::North; 4], 0); 16];
    let mut mask = 0;
    while mask < table.len() {
        let mut side = 0;
        while side < Direction::ALL.len() {
            if mask & (1 << side) != 0 {
                let count = table[mask].1;
                table[mask].0[count] = Direction::ALL[side];
                table[mask].1 = count + 1;
            }
            side += 1;
        }
        mask += 1;
    }
    table
};

/// The step towards each direction, indexed by `direction as usize`.
const OFFSETS: [(isize, isize); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// Fills a maze one cell wide or one cell tall: there is only one perfect
/// maze of that shape, the straight corridor, so it is carved directly.
///
/// Returns whether the maze had that shape. Every algorithm draws exactly this
/// corridor there, so taking the shortcut changes no maze; for the random
/// walks it is the difference between no time at all and days, because a walk
/// along a corridor takes about the square of its length to reach the end.
pub(crate) fn carve_corridor(map: &mut Map) -> bool {
    let (width, height) = (map.width() as usize, map.height() as usize);
    if height == 1 {
        for x in 1..width {
            map.carve(x - 1, 0, Direction::East);
        }
        true
    } else if width == 1 {
        for y in 1..height {
            map.carve(0, y - 1, Direction::South);
        }
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::random_step;
    use crate::map::Map;
    use crate::rng::Rng;

    /// The table-driven step has to pick the neighbour the general route
    /// picks, from the same draw, or every random-walk maze would change.
    #[test]
    fn a_random_step_is_the_neighbour_neighbor_at_picks() {
        for (w, h) in [(1, 1), (1, 5), (5, 1), (2, 2), (4, 3)] {
            let map = Map::new(w as u64, h as u64);
            for y in 0..h {
                for x in 0..w {
                    let (mut fast, mut plain) = (Rng::new(9), Rng::new(9));
                    for _ in 0..64 {
                        let count = map.neighbor_count(x, y);
                        if count == 0 {
                            break;
                        }
                        let expected = map.neighbor_at(x, y, plain.below(count as u64) as usize);
                        assert_eq!(
                            random_step(x, y, w, h, &mut fast),
                            expected,
                            "({x},{y}) in {w}x{h}"
                        );
                    }
                }
            }
        }
    }
}
