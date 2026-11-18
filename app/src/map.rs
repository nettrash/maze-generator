//! In-memory representation of a maze, plus its text file format.
//!
//! A maze is a grid of cells where every cell keeps a bitmask of the walls on
//! its four sides. By default the generator produces *perfect* mazes: the walls
//! form a spanning tree, so there is exactly one path between any two cells
//! (see the project README, which requires a single path from the top-left
//! corner to the bottom-right corner).
//!
//! # Variants
//!
//! Two optional shapes are supported on top of the perfect default, both
//! reachable through [`crate::Generator`] and its [`crate::MazeOptions`]:
//!
//! - *Braided* mazes have their dead ends knocked out with extra openings, so
//!   there are loops and more than one route between cells. See [`Map::braid`]
//!   and [`Map::dead_ends`].
//! - *Entrance and exit* openings punch a hole in the outer border at the
//!   start and end cells. See [`Map::open_entrance_and_exit`] and
//!   [`Map::is_border_open`].
//!
//! # File format
//!
//! [`Map::save_to_file`] writes a small human-readable text document and
//! [`Map::load_from_file`] reads it back:
//!
//! ```text
//! maze 2
//! 3 2
//! +--+--+--+
//! |        |
//! +  +  +  +
//! |  |     |
//! +--+  +--+
//! |        |
//! +--+--+--+
//! ```
//!
//! The first line is a magic marker and the format version, the second line is
//! `width height`, and the remaining `2 * height + 1` lines are the ASCII
//! rendering of the maze. The rendering is lossless: loading a saved maze
//! always reproduces the maze it came from, including its extra openings and
//! border holes.
//!
//! Version 1 of the format predates border openings and is still readable, but
//! a version 1 file with a hole in its outer border is rejected. Newly written
//! files use version 2.
//!
//! # Neighbours
//!
//! Three ways to walk a cell's neighbours, chosen by what the caller needs:
//!
//! - [`Map::each_neighbor`] iterates all of them, which is what the graph walks
//!   in [`Map::is_perfect`] and [`Map::solve`] do.
//! - [`Map::neighbor_count`] with [`Map::neighbor_at`] picks one at random
//!   without building anything, which is what the random-walk algorithms do.
//! - [`Map::neighbors`] hands back the packed array itself.
//!
//! All three agree, are allocation free, and count neighbours by position
//! rather than by walls: a cell keeps its neighbours whether or not the wall
//! between them still stands, since carving towards a walled neighbour is the
//! whole point of asking.
//!
//! # Solving
//!
//! [`Map::solve`] returns the unique route from the top-left corner to the
//! bottom-right corner, and [`Map::to_ascii_with_path`] renders the maze with
//! that route drawn onto it.
//!
//! # SVG export
//!
//! [`Map::to_svg`] and [`Map::to_svg_with_path`] render the maze as a vector
//! drawing for printing and sharing. It is a one-way export: only the text
//! format described above is read back.

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::fs;
use std::io::{BufWriter, Error, ErrorKind, Write};
use std::path::Path;

use crate::rng::Rng;

/// Wall bit set on the north side of a cell.
pub const WALL_NORTH: u8 = 0b0001;
/// Wall bit set on the east side of a cell.
pub const WALL_EAST: u8 = 0b0010;
/// Wall bit set on the south side of a cell.
pub const WALL_SOUTH: u8 = 0b0100;
/// Wall bit set on the west side of a cell.
pub const WALL_WEST: u8 = 0b1000;

/// All four walls, the state of a freshly created cell.
const WALLS_ALL: u8 = WALL_NORTH | WALL_EAST | WALL_SOUTH | WALL_WEST;

/// Magic marker at the start of the first line of a maze file.
const MAGIC: &str = "maze";
/// Version of the maze file format written by [`Map::save_to_file`].
///
/// Version 1 requires a solid outer border. Version 2 also allows the border to
/// be opened by [`Map::open_border`] for an entrance or an exit, so both are
/// read but only the current one is written.
const FORMAT_VERSION: u32 = 2;
/// Oldest file format version this crate can still read.
const MIN_FORMAT_VERSION: u32 = 1;

/// Number of characters a single cell occupies when rendered as ASCII art.
const CELL_WIDTH: usize = 3;

/// Size of one cell in SVG user units.
const SVG_CELL: f64 = 20.0;
/// Colour of the maze walls in the SVG rendering.
const SVG_WALL_COLOR: &str = "#000000";
/// Stroke width of the maze walls.
const SVG_WALL_WIDTH: f64 = 2.0;
/// Colour of a solution path drawn onto the SVG rendering.
const SVG_PATH_COLOR: &str = "#1a7f37";
/// Stroke width of a solution path, thinner than a wall so both stay visible.
const SVG_PATH_WIDTH: f64 = 6.0;

/// One of the four sides of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    /// Every direction, in clockwise order starting from the north.
    pub const ALL: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];

    /// The wall bit this direction refers to on a cell.
    pub fn wall_bit(self) -> u8 {
        match self {
            Direction::North => WALL_NORTH,
            Direction::East => WALL_EAST,
            Direction::South => WALL_SOUTH,
            Direction::West => WALL_WEST,
        }
    }

    /// The direction pointing back the other way.
    pub fn opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
        }
    }

    /// The `(dx, dy)` step from a cell towards this direction.
    pub fn offset(self) -> (i64, i64) {
        match self {
            Direction::North => (0, -1),
            Direction::East => (1, 0),
            Direction::South => (0, 1),
            Direction::West => (-1, 0),
        }
    }
}

/// Represents the maze map: a grid of walled cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    width: usize,
    height: usize,
    /// Row-major wall bitmasks: `grid[y * width + x]` holds the walls of the
    /// cell at column `x`, row `y`.
    ///
    /// One flat allocation rather than one `Vec` per row, so walking the maze
    /// in reading order stays cache friendly and every cell access is a single
    /// bounds check instead of two.
    grid: Vec<u8>,
}

impl Map {
    /// Creates a new maze map with the specified width and height.
    ///
    /// Every cell starts with all four walls intact, which is what the
    /// generation algorithms expect: they only ever knock walls down.
    pub fn new(width: u64, height: u64) -> Self {
        let width = width as usize;
        let height = height as usize;
        Map {
            width,
            height,
            grid: vec![WALLS_ALL; Self::cells(width, height)],
        }
    }

    /// Creates a maze of the given size with no walls at all.
    ///
    /// Used by the parser, which then raises exactly the walls found in the
    /// ASCII art. The public [`Map::new`] starts fully walled instead, which is
    /// what the generation algorithms expect.
    fn with_open_grid(width: usize, height: usize) -> Self {
        Map {
            width,
            height,
            grid: vec![0; Self::cells(width, height)],
        }
    }

    /// The length of the flat grid for a maze of this size.
    ///
    /// Saturating rather than wrapping: an absurd `width * height` should fail
    /// loudly when the grid is allocated, not wrap to a small length that every
    /// later cell access would panic on.
    fn cells(width: usize, height: usize) -> usize {
        width.saturating_mul(height)
    }

    /// The width of the maze in cells.
    pub fn width(&self) -> u64 {
        self.width as u64
    }

    /// The height of the maze in cells.
    pub fn height(&self) -> u64 {
        self.height as u64
    }

    /// Whether the maze has no cells at all.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    fn cell(&self, x: usize, y: usize) -> u8 {
        self.grid[y * self.width + x]
    }

    fn cell_mut(&mut self, x: usize, y: usize) -> &mut u8 {
        &mut self.grid[y * self.width + x]
    }

    /// Whether the given coordinates fall inside the maze.
    pub fn in_bounds(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }

    /// Whether a wall stands on the given side of a cell.
    ///
    /// A side that leaves the maze reports a wall unless it was opened by
    /// [`Map::open_border`], which is how an entrance or exit is represented.
    pub fn has_wall(&self, x: usize, y: usize, direction: Direction) -> bool {
        if x >= self.width || y >= self.height {
            return true;
        }
        self.cell(x, y) & direction.wall_bit() != 0
    }

    /// The neighbour of a cell in the given direction, or `None` when the
    /// direction steps outside the maze.
    pub fn neighbor(&self, x: usize, y: usize, direction: Direction) -> Option<(usize, usize)> {
        let (dx, dy) = direction.offset();
        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
        if self.in_bounds(nx, ny) {
            Some((nx as usize, ny as usize))
        } else {
            None
        }
    }

    /// The in-bounds neighbours of a cell together with the wall that separates
    /// them, used by the generation algorithms to pick where to carve next.
    ///
    /// The entries are packed at the front of the array and the rest are
    /// `None`, so [`Map::neighbor_count`] says how many are usable and the
    /// array can be indexed directly up to that count. It is a fixed array
    /// rather than a `Vec` because this is the hottest call in every algorithm:
    /// each of them asks for the neighbours of every cell it touches, and
    /// allocating a `Vec` each time showed up in the benchmarks.
    ///
    /// A cell on the border simply has fewer entries; a cell outside the maze
    /// has none.
    pub fn neighbors(&self, x: usize, y: usize) -> [Option<(usize, usize, Direction)>; 4] {
        let mut result = [None; 4];
        if x >= self.width || y >= self.height {
            return result;
        }
        let mut index = 0;
        for direction in Direction::ALL {
            if let Some(neighbor) = self.neighbor(x, y, direction) {
                result[index] = Some((neighbor.0, neighbor.1, direction));
                index += 1;
            }
        }
        result
    }

    /// How many neighbours a cell has, which is four everywhere but on the
    /// border.
    ///
    /// Cheaper than counting the entries of [`Map::neighbors`] because it only
    /// looks at the position, never at the walls. Algorithms that want to pick
    /// a neighbour at random ask for this and then index the array.
    pub fn neighbor_count(&self, x: usize, y: usize) -> usize {
        if x >= self.width || y >= self.height {
            return 0;
        }
        let mut count = 0;
        if y > 0 {
            count += 1;
        }
        if x + 1 < self.width {
            count += 1;
        }
        if y + 1 < self.height {
            count += 1;
        }
        if x > 0 {
            count += 1;
        }
        count
    }

    /// The `index`th neighbour of a cell, as returned by [`Map::neighbors`].
    ///
    /// `index` must be below [`Map::neighbor_count`]; the entries are packed at
    /// the front of the array, so this is the indexed form the random-walk
    /// algorithms want.
    ///
    /// # Panics
    ///
    /// Panics when `index` is not below [`Map::neighbor_count`].
    pub fn neighbor_at(&self, x: usize, y: usize, index: usize) -> (usize, usize, Direction) {
        self.neighbors(x, y)[index].expect("index within neighbor_count")
    }

    /// Iterates over every neighbour of a cell together with the wall that
    /// separates them.
    ///
    /// This is the convenient form when all neighbours are wanted, as the graph
    /// walks in [`Map::is_perfect`] and [`Map::solve`] do. To pick one at random
    /// without touching the walls, use [`Map::neighbor_count`] with
    /// [`Map::neighbor_at`]. A cell outside the maze yields nothing.
    pub fn each_neighbor(
        &self,
        x: usize,
        y: usize,
    ) -> impl Iterator<Item = (usize, usize, Direction)> {
        self.neighbors(x, y).into_iter().flatten()
    }

    /// Removes the wall between a cell and its neighbour, returning whether the
    /// wall could be removed at all (it cannot on the outer border).
    ///
    /// The wall is shared, so the matching wall on the neighbour is removed too.
    pub fn carve(&mut self, x: usize, y: usize, direction: Direction) -> bool {
        let Some((nx, ny)) = self.neighbor(x, y, direction) else {
            return false;
        };
        *self.cell_mut(x, y) &= !direction.wall_bit();
        *self.cell_mut(nx, ny) &= !direction.opposite().wall_bit();
        true
    }

    /// Knocks a hole in the outer border, returning whether there was a border
    /// wall there to remove.
    ///
    /// This is how an entrance or an exit is made: the maze stays enclosed
    /// everywhere else, but the given side of the given cell opens to the
    /// outside. The opening is drawn by [`Map::to_ascii`] and survives a save
    /// and load round trip.
    pub fn open_border(&mut self, x: usize, y: usize, direction: Direction) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let (dx, dy) = direction.offset();
        if self.in_bounds(x as i64 + dx, y as i64 + dy) {
            return false;
        }
        let had_wall = self.cell(x, y) & direction.wall_bit() != 0;
        *self.cell_mut(x, y) &= !direction.wall_bit();
        had_wall
    }

    /// Whether the outer border is open on the given side of a cell.
    pub fn is_border_open(&self, x: usize, y: usize, direction: Direction) -> bool {
        let (dx, dy) = direction.offset();
        if self.in_bounds(x as i64 + dx, y as i64 + dy) {
            return false;
        }
        !self.has_wall(x, y, direction)
    }

    /// Opens the conventional entrance and exit: the west side of the top-left
    /// cell and the east side of the bottom-right cell.
    ///
    /// Returns whether both openings were made, which fails only on an empty
    /// maze.
    pub fn open_entrance_and_exit(&mut self) -> bool {
        if self.is_empty() {
            return false;
        }
        let entrance = self.open_border(0, 0, Direction::West);
        let exit = self.open_border(self.width - 1, self.height - 1, Direction::East);
        entrance && exit
    }

    /// The cells with only one opening, which are the cul-de-sacs of the maze.
    pub fn dead_ends(&self) -> Vec<(usize, usize)> {
        let mut dead_ends = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.passage_count(x, y) == 1 {
                    dead_ends.push((x, y));
                }
            }
        }
        dead_ends
    }

    /// How many cells are dead ends.
    ///
    /// The same walk as [`Map::dead_ends`] without building the list, for
    /// callers that only want the number. On a maze with no walls that is the
    /// difference between a `Vec` of every cell and no allocation at all.
    pub fn dead_end_count(&self) -> usize {
        let mut count = 0;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.passage_count(x, y) == 1 {
                    count += 1;
                }
            }
        }
        count
    }

    /// How many of a cell's four sides are open, counting border openings.
    fn passage_count(&self, x: usize, y: usize) -> usize {
        Direction::ALL
            .iter()
            .filter(|direction| !self.has_wall(x, y, **direction))
            .count()
    }

    /// Removes a wall at every dead end that has one to remove, turning a
    /// perfect maze into a braided one with no avoidable cul-de-sacs.
    ///
    /// Each dead end is joined to one of its walled neighbours, which adds a
    /// loop rather than removing a route, so the maze stays connected and every
    /// cell stays reachable. Returns the number of walls removed.
    ///
    /// Only interior walls are touched: opening the outer border belongs to
    /// [`Map::open_border`], not to braiding. A dead end whose remaining walls
    /// are all border walls therefore cannot be opened, which happens in very
    /// narrow mazes such as a 2x1 corridor. [`Map::dead_ends`] still lists
    /// those cells afterwards.
    pub fn braid(&mut self, rng: &mut Rng) -> usize {
        let mut removed = 0;
        for (x, y) in self.dead_ends() {
            // Collect the candidates first: carving changes the maze, so the
            // list must not be built while it is being walked.
            let candidates: Vec<Direction> = Direction::ALL
                .iter()
                .copied()
                .filter(|direction| self.neighbor(x, y, *direction).is_some())
                .filter(|direction| self.has_wall(x, y, *direction))
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let direction = candidates[rng.below(candidates.len() as u64) as usize];
            if self.carve(x, y, direction) {
                removed += 1;
            }
        }
        removed
    }

    /// Rebuilds the wall between a cell and its neighbour.
    #[cfg(test)]
    fn raise(&mut self, x: usize, y: usize, direction: Direction) -> bool {
        let Some((nx, ny)) = self.neighbor(x, y, direction) else {
            return false;
        };
        *self.cell_mut(x, y) |= direction.wall_bit();
        *self.cell_mut(nx, ny) |= direction.opposite().wall_bit();
        true
    }

    /// Counts the openings between neighbouring cells, each counted once.
    fn openings(&self) -> usize {
        let mut count = 0;
        for y in 0..self.height {
            for x in 0..self.width {
                if x + 1 < self.width && self.cell(x, y) & WALL_EAST == 0 {
                    count += 1;
                }
                if y + 1 < self.height && self.cell(x, y) & WALL_SOUTH == 0 {
                    count += 1;
                }
            }
        }
        count
    }

    /// The cells reachable from the top-left corner by walking through removed
    /// walls, as a flat bitmap indexed by `y * width + x`.
    ///
    /// A bitmap rather than a `HashSet<(usize, usize)>`: the walk touches every
    /// cell in a full maze, and hashing a tuple per visit dominated the cost of
    /// [`Map::is_perfect`] and [`Map::is_solvable`]. The stack is a `Vec` used
    /// depth-first, since reachability does not care about the order.
    fn reachable_bitmap(&self) -> Vec<bool> {
        let cells = self.width * self.height;
        let mut seen = vec![false; cells];
        let mut stack = vec![0usize];
        seen[0] = true;
        while let Some(index) = stack.pop() {
            let (x, y) = (index % self.width, index / self.width);
            for (nx, ny, direction) in self.each_neighbor(x, y) {
                if self.has_wall(x, y, direction) {
                    continue;
                }
                let next = ny * self.width + nx;
                if !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        seen
    }

    /// How many cells can be reached from the top-left corner.
    ///
    /// Equal to the number of cells exactly when the maze is fully connected,
    /// which is what [`Map::is_perfect`] checks.
    pub fn reachable_count(&self) -> usize {
        if self.is_empty() {
            return 0;
        }
        self.reachable_bitmap().iter().filter(|&&seen| seen).count()
    }

    /// Whether the bottom-right corner can be reached from the top-left corner.
    pub fn is_solvable(&self) -> bool {
        if self.is_empty() {
            return false;
        }
        let goal = (self.height - 1) * self.width + (self.width - 1);
        self.reachable_bitmap()[goal]
    }

    /// Whether the maze is *perfect*: exactly one path connects any two cells.
    ///
    /// This holds when the openings form a spanning tree, which is checked by
    /// requiring one fewer opening than there are cells and every cell
    /// reachable from the origin.
    pub fn is_perfect(&self) -> bool {
        if self.is_empty() {
            return false;
        }
        let cells = self.width * self.height;
        self.openings() == cells - 1 && self.reachable_count() == cells
    }

    /// Renders the horizontal wall line above row `y`.
    ///
    /// Row `0` is the top border and row `height` the bottom border; both are
    /// solid except where [`Map::open_border`] made an opening.
    fn horizontal_line(&self, y: usize) -> String {
        let mut line = String::with_capacity(CELL_WIDTH * self.width + 1);
        line.push('+');
        for x in 0..self.width {
            let solid = match y {
                0 => self.has_wall(x, 0, Direction::North),
                y if y == self.height => self.has_wall(x, y - 1, Direction::South),
                y => self.cell(x, y - 1) & WALL_SOUTH != 0,
            };
            // An opened border is drawn as a gap, and the cell below it is
            // where a path marker belongs.
            let ch = if solid { '-' } else { ' ' };
            for _ in 0..CELL_WIDTH - 1 {
                line.push(ch);
            }
            line.push('+');
        }
        line
    }

    /// Renders the line of cell interiors for row `y`, including the vertical
    /// walls between them.
    ///
    /// When `path` is given, the cells it covers are drawn with a marker
    /// instead of blanks.
    fn vertical_line(&self, y: usize, path: Option<&PathMark>) -> String {
        let mut line = String::with_capacity(CELL_WIDTH * self.width + 1);
        line.push(if self.has_wall(0, y, Direction::West) {
            '|'
        } else {
            ' '
        });
        for x in 0..self.width {
            match path.and_then(|path| path.interior(x, y)) {
                Some(interior) => line.push_str(&interior),
                None => {
                    for _ in 0..CELL_WIDTH - 1 {
                        line.push(' ');
                    }
                }
            }
            line.push(if self.has_wall(x, y, Direction::East) {
                '|'
            } else {
                ' '
            });
        }
        line
    }

    /// Renders the maze as ASCII art built from `+`, `-` and `|`.
    pub fn to_ascii(&self) -> String {
        self.render(None)
    }

    /// Renders the maze with a solution path drawn onto it.
    ///
    /// Path cells are filled with `.`, the first cell of the path with `S` and
    /// the last with `E`. Walls are drawn exactly as [`Map::to_ascii`] draws
    /// them, so the two renderings differ only inside path cells.
    pub fn to_ascii_with_path(&self, path: &[(usize, usize)]) -> String {
        if path.is_empty() {
            return self.to_ascii();
        }
        self.render(Some(&PathMark::new(path)))
    }

    /// Renders the maze as SVG.
    ///
    /// Walls are drawn as strokes on a white background, one `<line>` per wall,
    /// so the output is small and scales to any size without losing quality.
    /// Returns an empty string for an empty maze.
    pub fn to_svg(&self) -> String {
        self.to_svg_with_path(&[])
    }

    /// Renders the maze as SVG with a solution path drawn underneath the walls.
    ///
    /// The path becomes a single rounded polyline through the centres of the
    /// given cells, drawn before the walls so the walls stay readable on top of
    /// it. An empty path renders the plain maze.
    pub fn to_svg_with_path(&self, path: &[(usize, usize)]) -> String {
        if self.is_empty() {
            return String::new();
        }

        let width = SVG_CELL * self.width as f64;
        let height = SVG_CELL * self.height as f64;

        let mut out = String::new();
        out.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
             viewBox=\"0 0 {width} {height}\">\n"
        ));
        out.push_str(&format!(
            "<rect width=\"{width}\" height=\"{height}\" fill=\"#ffffff\"/>\n"
        ));

        if !path.is_empty() {
            let points: Vec<String> = path
                .iter()
                .map(|(x, y)| {
                    format!(
                        "{},{}",
                        (*x as f64 + 0.5) * SVG_CELL,
                        (*y as f64 + 0.5) * SVG_CELL
                    )
                })
                .collect();
            out.push_str(&format!(
                "<polyline fill=\"none\" stroke=\"{SVG_PATH_COLOR}\" stroke-width=\"{SVG_PATH_WIDTH}\" \
                 stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\"{}\"/>\n",
                points.join(" ")
            ));
        }

        for y in 0..self.height {
            for x in 0..self.width {
                // Each wall is drawn from exactly one of its two cells: the
                // north and west walls from the cell that owns them, and the
                // east and south walls only from the border cells, so no wall
                // is drawn twice.
                if self.has_wall(x, y, Direction::North) {
                    push_segment(&mut out, x, y, x + 1, y);
                }
                if self.has_wall(x, y, Direction::West) {
                    push_segment(&mut out, x, y, x, y + 1);
                }
                if x + 1 == self.width && self.has_wall(x, y, Direction::East) {
                    push_segment(&mut out, x + 1, y, x + 1, y + 1);
                }
                if y + 1 == self.height && self.has_wall(x, y, Direction::South) {
                    push_segment(&mut out, x, y + 1, x + 1, y + 1);
                }
            }
        }

        out.push_str("</svg>\n");
        out
    }

    /// Shared implementation of the two renderings.
    fn render(&self, path: Option<&PathMark>) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for y in 0..self.height {
            out.push_str(&self.horizontal_line(y));
            out.push('\n');
            out.push_str(&self.vertical_line(y, path));
            out.push('\n');
        }
        out.push_str(&self.horizontal_line(self.height));
        out
    }

    /// The unique path from the top-left corner to the bottom-right corner.
    ///
    /// The generator produces perfect mazes, so this path is the only route
    /// between the two corners. It is returned as the list of cells to walk
    /// through, starting with the entrance cell and ending with the goal cell;
    /// a maze of width `w` and height `h` therefore has `w + h - 1` cells when
    /// solved by a straight walk, and more once the corridors wind.
    ///
    /// Returns `None` when the goal cannot be reached, which happens for a maze
    /// that was edited by hand or saved with a broken wall.
    pub fn solve(&self) -> Option<Vec<(usize, usize)>> {
        if self.is_empty() {
            return None;
        }
        self.path_between((0, 0), (self.width - 1, self.height - 1))
    }

    /// The shortest walk from `start` to `goal` through removed walls, or
    /// `None` when the two are not connected or either is outside the maze.
    ///
    /// Breadth-first search is used rather than a shortest-path algorithm
    /// because every step costs the same, so the first time a cell is reached
    /// is by a shortest route.
    pub fn path_between(
        &self,
        start: (usize, usize),
        goal: (usize, usize),
    ) -> Option<Vec<(usize, usize)>> {
        if !self.in_bounds(start.0 as i64, start.1 as i64)
            || !self.in_bounds(goal.0 as i64, goal.1 as i64)
        {
            return None;
        }
        if start == goal {
            return Some(vec![start]);
        }

        // Each reached cell records the cell it was reached from, which is how
        // the walk is rebuilt once the goal turns up. Indexed by cell rather
        // than keyed by coordinate: the walk can visit every cell, and hashing
        // a tuple per visit dominated the cost of this function.
        let cells = self.width * self.height;
        let unvisited = usize::MAX;
        let mut came_from = vec![unvisited; cells];
        let mut queue = VecDeque::new();
        let start_index = start.1 * self.width + start.0;
        let goal_index = goal.1 * self.width + goal.0;
        came_from[start_index] = start_index;
        queue.push_back(start_index);

        while let Some(index) = queue.pop_front() {
            let (x, y) = (index % self.width, index / self.width);
            for (nx, ny, direction) in self.each_neighbor(x, y) {
                if self.has_wall(x, y, direction) {
                    continue;
                }
                let next = ny * self.width + nx;
                if came_from[next] != unvisited {
                    continue;
                }
                came_from[next] = index;
                if next == goal_index {
                    return Some(self.rebuild_path(&came_from, start_index, goal_index));
                }
                queue.push_back(next);
            }
        }
        None
    }

    /// Walks the `came_from` table backwards from `goal` to `start`.
    fn rebuild_path(
        &self,
        came_from: &[usize],
        start_index: usize,
        goal_index: usize,
    ) -> Vec<(usize, usize)> {
        let mut path = vec![(goal_index % self.width, goal_index / self.width)];
        let mut current = goal_index;
        while current != start_index {
            current = came_from[current];
            path.push((current % self.width, current / self.width));
        }
        path.reverse();
        path
    }

    /// Parses a maze from the text format written by [`Map::save_to_file`].
    pub fn from_ascii(text: &str) -> Result<Map, Error> {
        let mut lines = text.lines();

        let header = lines.next().ok_or_else(|| invalid("missing header line"))?;
        let mut header_fields = header.split_whitespace();
        if header_fields.next() != Some(MAGIC) {
            return Err(invalid("file does not start with the 'maze' marker"));
        }
        let version: u32 = header_fields
            .next()
            .ok_or_else(|| invalid("missing format version in header line"))?
            .parse()
            .map_err(|_| invalid("format version is not a number"))?;
        if !(MIN_FORMAT_VERSION..=FORMAT_VERSION).contains(&version) {
            return Err(invalid(format!(
                "unsupported format version {version}, expected {MIN_FORMAT_VERSION} to {FORMAT_VERSION}"
            )));
        }
        // Only version 2 can represent an opening in the outer border, so a
        // version 1 file with a gap there is corrupt rather than open.
        let border_may_be_open = version >= 2;

        let dims = lines
            .next()
            .ok_or_else(|| invalid("missing dimensions line"))?;
        let mut dims_fields = dims.split_whitespace();
        let width: usize = dims_fields
            .next()
            .ok_or_else(|| invalid("missing width"))?
            .parse()
            .map_err(|_| invalid("width is not a number"))?;
        let height: usize = dims_fields
            .next()
            .ok_or_else(|| invalid("missing height"))?
            .parse()
            .map_err(|_| invalid("height is not a number"))?;
        if width == 0 || height == 0 {
            return Err(invalid("maze must have a positive width and height"));
        }
        if dims_fields.next().is_some() {
            return Err(invalid("too many values on the dimensions line"));
        }

        let mut map = Map::with_open_grid(width, height);
        let line_len = CELL_WIDTH * width + 1;

        for row in 0..(2 * height + 1) {
            let raw = lines
                .next()
                .ok_or_else(|| invalid(format!("missing art line {row}")))?;
            let chars: Vec<char> = raw.chars().collect();
            if chars.len() != line_len {
                return Err(invalid(format!(
                    "art line {row} has {} characters, expected {line_len}",
                    chars.len()
                )));
            }

            if row % 2 == 0 {
                let y = row / 2;
                let border = y == 0 || y == height;
                for x in 0..=width {
                    if chars[CELL_WIDTH * x] != '+' {
                        return Err(invalid(format!("art line {row}: '+' expected")));
                    }
                }
                for x in 0..width {
                    let a = chars[CELL_WIDTH * x + 1];
                    let b = chars[CELL_WIDTH * x + 2];
                    if a != b || !matches!(a, '-' | ' ') {
                        return Err(invalid(format!(
                            "art line {row}: unexpected wall drawing at column {}",
                            CELL_WIDTH * x + 1
                        )));
                    }
                    if border {
                        if a == '-' {
                            if y == 0 {
                                *map.cell_mut(x, 0) |= WALL_NORTH;
                            } else {
                                *map.cell_mut(x, height - 1) |= WALL_SOUTH;
                            }
                        } else if !border_may_be_open {
                            return Err(invalid(format!(
                                "art line {row}: format version {version} requires a solid outer border"
                            )));
                        }
                    } else if a == '-' {
                        *map.cell_mut(x, y - 1) |= WALL_SOUTH;
                        *map.cell_mut(x, y) |= WALL_NORTH;
                    }
                }
            } else {
                let y = row / 2;
                for x in 0..width {
                    for offset in 1..CELL_WIDTH {
                        if chars[CELL_WIDTH * x + offset] != ' ' {
                            return Err(invalid(format!(
                                "art line {row}: cell interiors must be blank"
                            )));
                        }
                    }
                }
                for x in 0..=width {
                    let ch = chars[CELL_WIDTH * x];
                    if x == 0 || x == width {
                        if ch == '|' {
                            if x == 0 {
                                *map.cell_mut(0, y) |= WALL_WEST;
                            } else {
                                *map.cell_mut(width - 1, y) |= WALL_EAST;
                            }
                        } else if ch != ' ' {
                            return Err(invalid(format!(
                                "art line {row}: unexpected character '{ch}'"
                            )));
                        } else if !border_may_be_open {
                            return Err(invalid(format!(
                                "art line {row}: format version {version} requires a solid outer border"
                            )));
                        }
                    } else if ch != '|' && ch != ' ' {
                        return Err(invalid(format!(
                            "art line {row}: unexpected character '{ch}'"
                        )));
                    } else if ch == '|' {
                        *map.cell_mut(x - 1, y) |= WALL_EAST;
                        *map.cell_mut(x, y) |= WALL_WEST;
                    }
                }
            }
        }

        for trailing in lines {
            if !trailing.trim().is_empty() {
                return Err(invalid("unexpected content after the maze"));
            }
        }

        Ok(map)
    }

    /// Renders the maze in the text format described at the top of this module.
    ///
    /// This is exactly what [`Map::save_to_file`] writes, and
    /// [`Map::from_ascii`] reads it back, so the two are inverses.
    pub fn to_text(&self) -> String {
        format!(
            "{MAGIC} {FORMAT_VERSION}\n{width} {height}\n{ascii}\n",
            width = self.width,
            height = self.height,
            ascii = self.to_ascii()
        )
    }

    /// Saves the maze to `path` in the text format described at the top of this
    /// module.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        if self.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "cannot save an empty maze",
            ));
        }
        let path = path.as_ref();
        let file = fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(self.to_text().as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    /// Loads a maze previously written by [`Map::save_to_file`].
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Map, Error> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .map_err(|e| Error::new(e.kind(), format!("{}: {e}", path.display())))?;
        Map::from_ascii(&text).map_err(|e| Error::new(e.kind(), format!("{}: {e}", path.display())))
    }

    /// Saves the maze to `path` as an SVG document.
    ///
    /// Unlike [`Map::save_to_file`] this is a one-way export: the SVG is meant
    /// for printing and sharing, and is not read back by this crate.
    pub fn save_svg_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        self.save_svg_to_file_with_path(path, &[])
    }

    /// Saves the maze to `path` as an SVG document with a solution path drawn
    /// onto it. See [`Map::save_svg_to_file`].
    pub fn save_svg_to_file_with_path<P: AsRef<Path>>(
        &self,
        path: P,
        route: &[(usize, usize)],
    ) -> Result<(), Error> {
        if self.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "cannot save an empty maze",
            ));
        }
        let path = path.as_ref();
        let file = fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        write!(writer, "{}", self.to_svg_with_path(route))?;
        writer.flush()?;
        Ok(())
    }
}

/// Which cells of a maze a rendered path covers, and how to draw them.
///
/// The path is stored as a set so rendering a row can ask about a cell without
/// scanning the path again.
struct PathMark {
    cells: HashSet<(usize, usize)>,
    start: (usize, usize),
    goal: (usize, usize),
}

impl PathMark {
    fn new(path: &[(usize, usize)]) -> Self {
        PathMark {
            cells: path.iter().copied().collect(),
            start: path[0],
            goal: path[path.len() - 1],
        }
    }

    /// The two interior characters of cell `(x, y)`, or `None` to leave it
    /// blank.
    ///
    /// Endpoints keep their letter and a leading space so they read as `S` and
    /// `E` rather than the doubled `SS` a repeated character would give.
    fn interior(&self, x: usize, y: usize) -> Option<String> {
        let cell = (x, y);
        if cell == self.start {
            Some("S ".to_string())
        } else if cell == self.goal {
            Some("E ".to_string())
        } else if self.cells.contains(&cell) {
            Some("..".to_string())
        } else {
            None
        }
    }
}

impl Default for Map {
    /// Provides a default maze map of size 100x100.
    fn default() -> Self {
        Map::new(100, 100)
    }
}

impl fmt::Display for Map {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_ascii())
    }
}

/// Appends one wall to `out` as a `<line>` between two cell corners.
fn push_segment(out: &mut String, x1: usize, y1: usize, x2: usize, y2: usize) {
    out.push_str(&format!(
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{SVG_WALL_COLOR}\" \
         stroke-width=\"{SVG_WALL_WIDTH}\"/>\n",
        x1 as f64 * SVG_CELL,
        y1 as f64 * SVG_CELL,
        x2 as f64 * SVG_CELL,
        y2 as f64 * SVG_CELL,
    ));
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Builds a path for a throwaway file that is always removed again.
    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "maze-test-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        path
    }

    #[test]
    fn neighbors_packs_its_entries_at_the_front_of_the_array() {
        // The indexed access the algorithms rely on is only correct while the
        // Some entries come before the None ones.
        let map = Map::new(3, 3);
        for y in 0..3 {
            for x in 0..3 {
                let neighbors = map.neighbors(x, y);
                let count = neighbors.iter().filter(|n| n.is_some()).count();
                assert!(
                    neighbors[..count].iter().all(Option::is_some),
                    "({x},{y}) has a gap before its {count} entries"
                );
                assert!(
                    neighbors[count..].iter().all(Option::is_none),
                    "({x},{y}) has an entry after its {count} entries"
                );
            }
        }
    }

    #[test]
    fn neighbor_count_agrees_with_the_packed_array_everywhere() {
        let map = Map::new(4, 3);
        for y in 0..3 {
            for x in 0..4 {
                let count = map.neighbor_count(x, y);
                let packed = map.neighbors(x, y);
                assert_eq!(
                    count,
                    packed.iter().filter(|n| n.is_some()).count(),
                    "neighbor_count disagrees with neighbors at ({x},{y})"
                );
                for (index, entry) in packed.iter().enumerate().take(count) {
                    assert_eq!(
                        map.neighbor_at(x, y, index),
                        entry.expect("packed entry"),
                        "neighbor_at disagrees with neighbors at ({x},{y})[{index}]"
                    );
                }
            }
        }
    }

    #[test]
    fn neighbor_count_follows_the_position_not_the_walls() {
        // A cell keeps its neighbours whether or not the walls are standing,
        // which is what lets the algorithms carve towards a walled neighbour.
        let walled = Map::new(3, 3);
        let mut open = Map::with_open_grid(3, 3);
        assert!(open.carve(1, 1, Direction::North));
        for (map, label) in [(&walled, "walled"), (&open, "open")] {
            assert_eq!(map.neighbor_count(1, 1), 4, "{label} centre");
            assert_eq!(map.neighbor_count(0, 0), 2, "{label} corner");
            assert_eq!(map.neighbor_count(1, 0), 3, "{label} edge");
        }
    }

    #[test]
    fn neighbors_of_a_cell_outside_the_maze_is_empty() {
        let map = Map::new(2, 2);
        assert_eq!(map.neighbor_count(2, 0), 0);
        assert_eq!(map.neighbor_count(0, 2), 0);
        assert!(map.neighbors(5, 5).iter().all(Option::is_none));
        assert_eq!(map.each_neighbor(5, 5).count(), 0);
    }

    #[test]
    fn each_neighbor_yields_the_packed_entries_in_order() {
        let map = Map::new(3, 3);
        // Corners list two neighbours, and they come in Direction::ALL order.
        assert_eq!(
            map.each_neighbor(0, 0).collect::<Vec<_>>(),
            vec![(1, 0, Direction::East), (0, 1, Direction::South)]
        );
        assert_eq!(
            map.each_neighbor(2, 2).collect::<Vec<_>>(),
            vec![(2, 1, Direction::North), (1, 2, Direction::West)]
        );
        for y in 0..3 {
            for x in 0..3 {
                assert_eq!(map.each_neighbor(x, y).count(), map.neighbor_count(x, y));
            }
        }
    }

    #[test]
    fn new_maze_keeps_every_wall_in_place() {
        let map = Map::new(3, 2);
        assert_eq!(map.width(), 3);
        assert_eq!(map.height(), 2);
        for y in 0..2 {
            for x in 0..3 {
                for direction in Direction::ALL {
                    assert!(map.has_wall(x, y, direction));
                }
            }
        }
        assert!(!map.is_perfect());
    }

    #[test]
    fn carve_removes_the_shared_wall_on_both_sides() {
        let mut map = Map::new(2, 1);
        assert!(map.carve(0, 0, Direction::East));
        assert!(!map.has_wall(0, 0, Direction::East));
        assert!(!map.has_wall(1, 0, Direction::West));
    }

    #[test]
    fn carve_cannot_break_the_outer_border() {
        let mut map = Map::new(2, 2);
        assert!(!map.carve(0, 0, Direction::North));
        assert!(!map.carve(0, 0, Direction::West));
        assert!(!map.carve(1, 1, Direction::East));
        assert!(!map.carve(1, 1, Direction::South));
        assert!(map.has_wall(0, 0, Direction::North));
    }

    #[test]
    fn default_maze_is_100x100() {
        let map = Map::default();
        assert_eq!(map.width(), 100);
        assert_eq!(map.height(), 100);
    }

    #[test]
    fn renders_a_fully_walled_maze() {
        let map = Map::new(2, 2);
        assert_eq!(
            map.to_ascii(),
            "+--+--+
|  |  |
+--+--+
|  |  |
+--+--+"
        );
    }

    #[test]
    fn renders_carved_walls_as_gaps() {
        let mut map = Map::new(2, 1);
        map.carve(0, 0, Direction::East);
        assert_eq!(
            map.to_ascii(),
            "+--+--+
|     |
+--+--+"
        );
    }

    #[test]
    fn save_and_load_round_trips() {
        let mut map = Map::new(6, 4);
        map.carve(0, 0, Direction::East);
        map.carve(3, 1, Direction::South);
        map.carve(5, 3, Direction::West);
        map.carve(2, 2, Direction::North);

        let path = temp_path("roundtrip.maze");
        map.save_to_file(&path).expect("save failed");
        let loaded = Map::load_from_file(&path).expect("load failed");
        let _ = fs::remove_file(&path);

        assert_eq!(loaded, map);
        assert_eq!(loaded.to_ascii(), map.to_ascii());
    }

    /// `to_text` is what `save_to_file` writes, so the two must agree exactly.
    #[test]
    fn to_text_is_what_the_saved_file_contains() {
        let map = perfect_maze(4);
        let path = temp_path("to_text.maze");
        map.save_to_file(&path).expect("save failed");
        let text = fs::read_to_string(&path).expect("read failed");
        let _ = fs::remove_file(&path);

        assert_eq!(text, map.to_text());
    }

    #[test]
    fn saved_file_starts_with_the_header() {
        let map = Map::new(3, 2);
        let path = temp_path("header.maze");
        map.save_to_file(&path).expect("save failed");
        let text = fs::read_to_string(&path).expect("read failed");
        let _ = fs::remove_file(&path);

        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("maze 2"));
        assert_eq!(lines.next(), Some("3 2"));
    }

    #[test]
    fn load_reports_missing_file() {
        let err = Map::load_from_file("/definitely/not/a/maze/file").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn load_rejects_a_file_without_the_marker() {
        let path = temp_path("bad-marker.maze");
        fs::write(&path, "nope 1\n2 1\n+--+--+\n|  |  |\n+--+--+\n").unwrap();
        let err = Map::load_from_file(&path).unwrap_err();
        let _ = fs::remove_file(&path);
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn load_rejects_a_broken_border() {
        let path = temp_path("broken-border.maze");
        fs::write(
            &path,
            "maze 1\n2 1\n+--+--+
   |  |
+--+--+\n",
        )
        .unwrap();
        let err = Map::load_from_file(&path).unwrap_err();
        let _ = fs::remove_file(&path);
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn load_rejects_wrong_line_length() {
        let path = temp_path("short-line.maze");
        fs::write(&path, "maze 1\n2 1\n+--+--+\n|  |  |\n+--+-+\n").unwrap();
        let err = Map::load_from_file(&path).unwrap_err();
        let _ = fs::remove_file(&path);
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn load_rejects_degenerate_dimensions() {
        let path = temp_path("zero.maze");
        fs::write(&path, "maze 1\n0 1\n").unwrap();
        let err = Map::load_from_file(&path).unwrap_err();
        let _ = fs::remove_file(&path);
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn save_refuses_an_empty_maze() {
        let map = Map::new(0, 3);
        let path = temp_path("empty.maze");
        let err = map.save_to_file(&path).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn a_spanning_tree_is_perfect_and_solvable() {
        // A 3x1 maze with both internal walls removed is a single corridor.
        let mut map = Map::new(3, 1);
        map.carve(0, 0, Direction::East);
        map.carve(1, 0, Direction::East);
        assert!(map.is_perfect());
        assert!(map.is_solvable());
    }

    #[test]
    fn a_disconnected_maze_is_not_perfect() {
        let mut map = Map::new(2, 1);
        map.carve(0, 0, Direction::East);
        map.raise(0, 0, Direction::East);
        assert!(!map.is_perfect());
        assert!(!map.is_solvable());
    }

    #[test]
    fn an_extra_loop_breaks_perfection() {
        // A 2x2 open box has four cells but four openings: one path too many.
        let mut map = Map::new(2, 2);
        map.carve(0, 0, Direction::East);
        map.carve(0, 0, Direction::South);
        map.carve(1, 0, Direction::South);
        map.carve(0, 1, Direction::East);
        assert!(map.is_solvable());
        assert!(!map.is_perfect());
    }

    #[test]
    fn empty_maze_is_neither_solvable_nor_perfect() {
        let map = Map::new(0, 0);
        assert!(map.is_empty());
        assert!(!map.is_solvable());
        assert!(!map.is_perfect());
        assert_eq!(map.to_ascii(), "");
    }

    /// A perfect maze of a size big enough to have real dead ends.
    fn perfect_maze(seed: u64) -> Map {
        crate::Generator::new(12, 9, "backtracking", Some(seed))
            .expect("known algorithm")
            .build()
    }

    #[test]
    fn dead_end_counts_agree_with_the_list() {
        // The counting walk must not drift from the listing one: they are the
        // same iterator, but `check` uses the count and tests use the list.
        for name in crate::ALGORITHMS {
            for seed in [1, 2, 3] {
                let map = crate::Generator::new(13, 10, name, Some(seed))
                    .expect("known algorithm")
                    .build();
                assert_eq!(
                    map.dead_end_count(),
                    map.dead_ends().len(),
                    "{name} at seed {seed}"
                );
            }
        }
        // A corridor has exactly two cul-de-sacs, its ends.
        let mut map = Map::new(3, 1);
        map.carve(0, 0, Direction::East);
        map.carve(1, 0, Direction::East);
        assert_eq!(map.dead_end_count(), 2);
        assert_eq!(map.dead_ends(), vec![(0, 0), (2, 0)]);
        // With no walls at all nothing is a cul-de-sac.
        let open = Map::with_open_grid(3, 3);
        assert_eq!(open.dead_end_count(), 0);
        assert!(open.dead_ends().is_empty());
    }

    #[test]
    fn a_plain_maze_keeps_its_dead_ends_and_its_border() {
        let map = perfect_maze(4);
        assert!(!map.dead_ends().is_empty(), "the fixture has no dead ends");
        for y in 0..map.height() as usize {
            assert!(!map.is_border_open(0, y, Direction::West));
            assert!(!map.is_border_open(map.width() as usize - 1, y, Direction::East));
        }
    }

    #[test]
    fn braiding_removes_every_dead_end() {
        let mut map = perfect_maze(4);
        let before = map.dead_ends().len();
        assert!(before > 0);

        let removed = map.braid(&mut Rng::new(1));
        assert!(removed > 0, "braiding should remove some walls");
        assert_eq!(
            map.dead_ends().len(),
            0,
            "a braided maze should have no cul-de-sacs left"
        );
    }

    #[test]
    fn braiding_keeps_the_maze_connected_and_solvable() {
        let mut map = perfect_maze(9);
        map.braid(&mut Rng::new(2));
        assert!(map.is_solvable(), "braiding cut the exit off");
        assert!(map.solve().is_some());
        // Braiding only adds openings, so every cell is still reachable.
        assert_eq!(map.reachable_count(), 12 * 9);
    }

    #[test]
    fn braiding_adds_loops_so_the_maze_is_no_longer_perfect() {
        let mut map = perfect_maze(9);
        assert!(map.is_perfect());
        map.braid(&mut Rng::new(3));
        assert!(!map.is_perfect(), "braiding should have added a loop");
    }

    #[test]
    fn braiding_is_reproducible_from_its_seed() {
        let braid = |seed: u64| {
            let mut map = perfect_maze(9);
            map.braid(&mut Rng::new(seed));
            map.to_ascii()
        };
        assert_eq!(braid(5), braid(5));
        assert_ne!(braid(5), braid(6));
    }

    #[test]
    fn a_single_cell_maze_braids_into_itself() {
        // Its four sides are all border walls, so it has no passage at all and
        // is not a cul-de-sac. There is nothing to join it to either, so
        // braiding must be a no-op rather than a panic.
        let mut map = Map::new(1, 1);
        assert!(map.dead_ends().is_empty());
        assert_eq!(map.braid(&mut Rng::new(1)), 0);
    }

    #[test]
    fn entrance_and_exit_open_the_border() {
        let mut map = perfect_maze(4);
        assert!(map.open_entrance_and_exit());
        assert!(map.is_border_open(0, 0, Direction::West));
        assert!(map.is_border_open(11, 8, Direction::East));
        // Everywhere else stays enclosed.
        assert!(!map.is_border_open(0, 0, Direction::North));
        assert!(!map.is_border_open(11, 8, Direction::South));
        assert!(!map.is_border_open(5, 0, Direction::North));
    }

    #[test]
    fn opening_the_border_renders_a_gap() {
        let mut map = Map::new(2, 1);
        map.carve(0, 0, Direction::East);
        assert!(map.open_border(0, 0, Direction::West));
        // The leading border wall is gone, and the shared wall was already
        // carved, so only the right border remains on that line.
        assert_eq!(
            map.to_ascii(),
            "+--+--+
      |
+--+--+"
        );
    }

    #[test]
    fn open_border_only_works_on_the_border() {
        let mut map = Map::new(3, 3);
        // An interior side is a shared wall, not a border.
        assert!(!map.open_border(1, 1, Direction::East));
        assert!(map.has_wall(1, 1, Direction::East));
        // Out of bounds cells cannot be opened either.
        assert!(!map.open_border(9, 9, Direction::North));
        assert!(map.open_border(1, 0, Direction::North));
        // Opening twice reports there was nothing left to open.
        assert!(!map.open_border(1, 0, Direction::North));
    }

    #[test]
    fn a_border_opening_survives_a_save_and_load_round_trip() {
        let mut map = perfect_maze(4);
        map.open_entrance_and_exit();
        let path = temp_path("open-border.maze");
        map.save_to_file(&path).expect("save failed");
        let loaded = Map::load_from_file(&path).expect("load failed");
        let _ = fs::remove_file(&path);

        assert_eq!(loaded.to_ascii(), map.to_ascii());
        assert!(loaded.is_border_open(0, 0, Direction::West));
        assert!(loaded.is_border_open(11, 8, Direction::East));
    }

    #[test]
    fn a_braided_maze_survives_a_save_and_load_round_trip() {
        let mut map = perfect_maze(4);
        map.braid(&mut Rng::new(7));
        let path = temp_path("braided.maze");
        map.save_to_file(&path).expect("save failed");
        let loaded = Map::load_from_file(&path).expect("load failed");
        let _ = fs::remove_file(&path);

        assert_eq!(loaded.to_ascii(), map.to_ascii());
        assert_eq!(loaded.dead_ends().len(), 0);
        assert!(loaded.is_solvable());
    }

    #[test]
    fn version_one_files_still_load_when_the_border_is_solid() {
        let path = temp_path("v1.maze");
        fs::write(
            &path,
            "maze 1\n2 1\n+--+--+
|     |
+--+--+\n",
        )
        .unwrap();
        let map = Map::load_from_file(&path).expect("a v1 file should still load");
        let _ = fs::remove_file(&path);
        assert_eq!(map.width(), 2);
        assert!(!map.is_border_open(0, 0, Direction::West));
    }

    #[test]
    fn version_one_files_reject_an_open_border() {
        // Version 1 has no way to represent an opening, so a gap there is
        // corrupt data rather than an entrance.
        let path = temp_path("v1-open.maze");
        fs::write(
            &path,
            "maze 1\n2 1\n+--+--+
    |  |
+--+--+\n",
        )
        .unwrap();
        let err = Map::load_from_file(&path).unwrap_err();
        let _ = fs::remove_file(&path);
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn an_unsupported_version_is_reported() {
        let path = temp_path("v9.maze");
        fs::write(&path, "maze 9\n1 1\n+--+\n|  |\n+--+\n").unwrap();
        let err = Map::load_from_file(&path).unwrap_err();
        let _ = fs::remove_file(&path);
        assert!(err.to_string().contains("unsupported format version"));
    }

    #[test]
    fn the_generator_applies_the_requested_variants() {
        let plain = crate::Generator::new(12, 9, "eller", Some(4))
            .expect("known algorithm")
            .build();
        assert!(!plain.dead_ends().is_empty());
        assert!(!plain.is_border_open(0, 0, Direction::West));

        let braided = crate::Generator::new(12, 9, "eller", Some(4))
            .expect("known algorithm")
            .with_options(crate::MazeOptions {
                braid: true,
                entrance_exit: false,
            })
            .build();
        assert_eq!(braided.dead_ends().len(), 0);

        let open = crate::Generator::new(12, 9, "eller", Some(4))
            .expect("known algorithm")
            .with_options(crate::MazeOptions {
                braid: false,
                entrance_exit: true,
            })
            .build();
        assert!(open.is_border_open(0, 0, Direction::West));
        assert!(open.is_border_open(11, 8, Direction::East));
        assert!(!open.dead_ends().is_empty(), "braiding should not have run");
    }

    #[test]
    fn a_braided_maze_from_the_generator_is_reproducible() {
        let build = || {
            crate::Generator::new(12, 9, "wilson", Some(6))
                .expect("known algorithm")
                .with_options(crate::MazeOptions {
                    braid: true,
                    entrance_exit: true,
                })
                .build()
                .to_ascii()
        };
        assert_eq!(build(), build());
    }

    /// Walks a 3x2 maze along the bottom row and up the right column, which is
    /// the only route between the corners.
    fn three_by_two_corridor() -> Map {
        let mut map = Map::new(3, 2);
        map.carve(0, 0, Direction::South);
        map.carve(0, 1, Direction::East);
        map.carve(1, 1, Direction::East);
        map
    }

    #[test]
    fn solve_finds_the_known_route() {
        let map = three_by_two_corridor();
        assert_eq!(map.solve(), Some(vec![(0, 0), (0, 1), (1, 1), (2, 1)]));
    }

    #[test]
    fn a_single_cell_maze_is_solved_by_standing_still() {
        let map = Map::new(1, 1);
        assert_eq!(map.solve(), Some(vec![(0, 0)]));
    }

    #[test]
    fn an_unreachable_exit_has_no_route() {
        // The exit is sealed off: nothing is carved next to it.
        let mut map = Map::new(3, 3);
        map.carve(0, 0, Direction::East);
        map.carve(1, 0, Direction::South);
        assert!(!map.is_solvable());
        assert_eq!(map.solve(), None);
    }

    #[test]
    fn path_between_reports_out_of_bounds_endpoints() {
        let map = three_by_two_corridor();
        assert_eq!(map.path_between((0, 0), (9, 9)), None);
        assert_eq!(map.path_between((9, 9), (0, 0)), None);
    }

    #[test]
    fn path_between_a_cell_and_itself_is_that_cell() {
        let map = three_by_two_corridor();
        assert_eq!(map.path_between((1, 1), (1, 1)), Some(vec![(1, 1)]));
    }

    /// Every step of a returned route must cross a removed wall, otherwise the
    /// solver is drawing a line through the maze rather than walking it.
    fn assert_route_is_walkable(map: &Map, route: &[(usize, usize)]) {
        for pair in route.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let dx = to.0 as i64 - from.0 as i64;
            let dy = to.1 as i64 - from.1 as i64;
            let direction = match (dx, dy) {
                (-1, 0) => Direction::West,
                (1, 0) => Direction::East,
                (0, -1) => Direction::North,
                (0, 1) => Direction::South,
                _ => panic!("{from:?} and {to:?} are not neighbours"),
            };
            assert!(
                !map.has_wall(from.0, from.1, direction),
                "the route crosses a wall between {from:?} and {to:?}"
            );
        }
    }

    #[test]
    fn solved_routes_only_walk_through_openings() {
        for name in crate::ALGORITHMS {
            let generator = crate::Generator::new(14, 11, name, Some(3)).expect("known algorithm");
            let map = generator.algorithm.generate_maze(14, 11);
            let route = map.solve().expect("a perfect maze is solvable");
            assert_eq!(route.first(), Some(&(0, 0)), "{name} start");
            assert_eq!(route.last(), Some(&(13, 10)), "{name} goal");
            assert_route_is_walkable(&map, &route);
        }
    }

    /// Exhaustively finds the shortest walkable route by depth-first search,
    /// independent of the breadth-first solver being tested.
    fn brute_force_shortest(map: &Map, goal: (usize, usize)) -> usize {
        fn dfs(
            map: &Map,
            cell: (usize, usize),
            goal: (usize, usize),
            steps: usize,
            visited: &mut Vec<bool>,
            best: &mut usize,
        ) {
            if steps >= *best {
                return;
            }
            if cell == goal {
                *best = steps;
                return;
            }
            let index = cell.1 * map.width() as usize + cell.0;
            visited[index] = true;
            for (nx, ny, direction) in map.each_neighbor(cell.0, cell.1) {
                if map.has_wall(cell.0, cell.1, direction) {
                    continue;
                }
                let next = ny * map.width() as usize + nx;
                if !visited[next] {
                    dfs(map, (nx, ny), goal, steps + 1, visited, best);
                }
            }
            visited[index] = false;
        }

        let mut best = usize::MAX;
        let mut visited = vec![false; map.width() as usize * map.height() as usize];
        dfs(map, (0, 0), goal, 0, &mut visited, &mut best);
        best
    }

    #[test]
    fn solved_routes_are_the_shortest_possible() {
        // The solver is breadth-first, so its route must be shortest even where
        // a maze has loops and several routes exist. Brute force agrees on
        // every algorithm, plain and braided.
        for name in crate::ALGORITHMS {
            for braid in [false, true] {
                let generator = crate::Generator::new(9, 7, name, Some(5))
                    .expect("known algorithm")
                    .with_options(crate::MazeOptions {
                        braid,
                        entrance_exit: false,
                    });
                let map = generator.build();
                let goal = (8, 6);
                let route = map.solve().expect("generated mazes are solvable");
                let expected = brute_force_shortest(&map, goal);
                assert_eq!(
                    route.len(),
                    expected + 1,
                    "{name} (braid {braid}) is not a shortest route"
                );
            }
        }
    }

    #[test]
    fn a_solved_route_never_visits_a_cell_twice() {
        let generator = crate::Generator::new(15, 12, "eller", Some(9)).expect("known algorithm");
        let map = generator.algorithm.generate_maze(15, 12);
        let route = map.solve().expect("a perfect maze is solvable");
        let unique: HashSet<(usize, usize)> = route.iter().copied().collect();
        assert_eq!(unique.len(), route.len(), "the route contains a cycle");
    }

    #[test]
    fn rendering_a_path_marks_the_route() {
        let map = three_by_two_corridor();
        let route = map.solve().expect("the corridor is solvable");
        assert_eq!(
            map.to_ascii_with_path(&route),
            "+--+--+--+
|S |  |  |
+  +--+--+
|.. .. E |
+--+--+--+"
        );
    }

    #[test]
    fn rendering_an_empty_path_is_the_plain_maze() {
        let map = three_by_two_corridor();
        assert_eq!(map.to_ascii_with_path(&[]), map.to_ascii());
    }

    /// Counts the wall segments the SVG renderer is expected to draw.
    ///
    /// A wall is drawn by exactly one owner: the cell to its south owns a
    /// horizontal wall, and the cell to its east owns a vertical one, except on
    /// the far border where there is no such cell.
    fn expected_segment_count(map: &Map) -> usize {
        let (width, height) = (map.width() as usize, map.height() as usize);
        let mut count = 0;
        for y in 0..height {
            for x in 0..width {
                if map.has_wall(x, y, Direction::North) {
                    count += 1;
                }
                if map.has_wall(x, y, Direction::West) {
                    count += 1;
                }
                if x + 1 == width && map.has_wall(x, y, Direction::East) {
                    count += 1;
                }
                if y + 1 == height && map.has_wall(x, y, Direction::South) {
                    count += 1;
                }
            }
        }
        count
    }

    /// Checks that every tag opened in `svg` is closed again, which is all the
    /// well-formedness a hand-written renderer can be trusted for.
    fn assert_well_formed(svg: &str) {
        assert!(
            svg.starts_with("<svg "),
            "the document must start with the root tag"
        );
        assert!(
            svg.trim_end().ends_with("</svg>"),
            "the root tag must be closed"
        );
        let mut open_tags = Vec::new();
        for chunk in svg.split('<').skip(1) {
            let chunk = chunk.trim_start();
            let tag_end = chunk
                .find(char::is_whitespace)
                .unwrap_or_else(|| chunk.find(['>', '/']).unwrap_or(chunk.len()));
            let name = chunk[..tag_end].trim_end_matches(['>', '/']);
            if name.starts_with('/') {
                let closed = name.trim_start_matches('/');
                assert_eq!(
                    open_tags.pop().as_deref(),
                    Some(closed),
                    "unexpected closing tag {closed}"
                );
            } else {
                let self_closing = chunk[..chunk.find('>').unwrap_or(chunk.len())]
                    .trim_end()
                    .ends_with('/');
                if !self_closing {
                    open_tags.push(name.to_string());
                }
            }
        }
        assert!(open_tags.is_empty(), "unclosed tags: {open_tags:?}");
    }

    #[test]
    fn svg_draws_one_line_per_wall() {
        let map = Map::new(3, 2);
        let svg = map.to_svg();
        assert_eq!(svg.matches("<line ").count(), expected_segment_count(&map));
        // A fully walled 3x2 maze has every interior wall plus both borders.
        assert_eq!(svg.matches("<line ").count(), 17);
        assert_well_formed(&svg);
    }

    #[test]
    fn removing_a_wall_removes_exactly_one_svg_segment() {
        let mut map = Map::new(4, 3);
        let before = map.to_svg().matches("<line ").count();
        assert!(map.carve(1, 1, Direction::East));
        let after = map.to_svg().matches("<line ").count();
        assert_eq!(after, before - 1, "a carved wall should disappear once");

        let mut map = Map::new(4, 3);
        assert!(map.carve(1, 1, Direction::South));
        assert_eq!(
            map.to_svg().matches("<line ").count(),
            before - 1,
            "a carved horizontal wall should disappear once too"
        );
    }

    #[test]
    fn svg_size_follows_the_maze_dimensions() {
        let svg = Map::new(5, 3).to_svg();
        assert!(
            svg.contains("width=\"100\" height=\"60\""),
            "expected the document to be 5x3 cells wide: {svg}"
        );
        assert!(svg.contains("viewBox=\"0 0 100 60\""));
    }

    #[test]
    fn svg_has_a_white_background() {
        let svg = Map::new(2, 2).to_svg();
        assert!(svg.contains("fill=\"#ffffff\""), "{svg}");
    }

    #[test]
    fn svg_path_is_a_single_polyline_through_the_cell_centres() {
        let map = three_by_two_corridor();
        let route = map.solve().expect("the corridor is solvable");
        let svg = map.to_svg_with_path(&route);
        assert_eq!(svg.matches("<polyline ").count(), 1);
        // One point per cell on the route, and the route starts at the centre
        // of cell (0, 0).
        let points = svg
            .split("points=\"")
            .nth(1)
            .expect("a points attribute")
            .split('"')
            .next()
            .expect("a terminated points attribute");
        assert_eq!(points.split(' ').count(), route.len());
        assert!(
            points.starts_with("10,10 "),
            "cell centres should be used: {points}"
        );
        assert_well_formed(&svg);
    }

    #[test]
    fn svg_without_a_path_has_no_polyline() {
        let map = three_by_two_corridor();
        assert!(!map.to_svg().contains("<polyline "));
        assert_eq!(map.to_svg_with_path(&[]), map.to_svg());
    }

    #[test]
    fn an_empty_maze_renders_no_svg() {
        assert_eq!(Map::new(0, 0).to_svg(), "");
    }

    #[test]
    fn save_svg_writes_the_same_document_to_disk() {
        let map = three_by_two_corridor();
        let path = temp_path("out.svg");
        map.save_svg_to_file(&path).expect("saving should work");
        let written = std::fs::read_to_string(&path).expect("the file should exist");
        assert_eq!(written, map.to_svg());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_svg_refuses_an_empty_maze() {
        let map = Map::new(0, 0);
        let error = map
            .save_svg_to_file(temp_path("empty.svg"))
            .expect_err("an empty maze cannot be saved");
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn rendering_a_path_leaves_the_walls_untouched() {
        let map = three_by_two_corridor();
        let route = map.solve().expect("the corridor is solvable");
        let plain_text = map.to_ascii();
        let marked_text = map.to_ascii_with_path(&route);
        let plain: Vec<&str> = plain_text.lines().collect();
        let marked: Vec<&str> = marked_text.lines().collect();
        assert_eq!(plain.len(), marked.len());
        for (index, (plain, marked)) in plain.iter().zip(&marked).enumerate() {
            if index % 2 == 0 {
                assert_eq!(plain, marked, "wall line {index} changed");
            } else {
                for (column, (plain, marked)) in plain.chars().zip(marked.chars()).enumerate() {
                    if column % CELL_WIDTH == 0 {
                        assert_eq!(plain, marked, "vertical wall at {index}:{column} changed");
                    }
                }
            }
        }
    }
}
