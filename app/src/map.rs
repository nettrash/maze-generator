//! In-memory representation of a maze, plus its text file format.
//!
//! A maze is a grid of cells where every cell keeps a bitmask of the walls on
//! its four sides. The generator produces *perfect* mazes: the walls form a
//! spanning tree, so there is exactly one path between any two cells (see the
//! project README, which requires a single path from the top-left corner to the
//! bottom-right corner).
//!
//! # File format
//!
//! [`Map::save_to_file`] writes a small human-readable text document and
//! [`Map::load_from_file`] reads it back:
//!
//! ```text
//! maze 1
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
//! always reproduces the maze it came from.

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::fs;
use std::io::{BufWriter, Error, ErrorKind, Write};
use std::path::Path;

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
const FORMAT_VERSION: u32 = 1;

/// Number of characters a single cell occupies when rendered as ASCII art.
const CELL_WIDTH: usize = 3;

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
    /// `grid[y][x]` holds the wall bitmask of the cell at column `x`, row `y`.
    grid: Vec<Vec<u8>>,
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
            grid: vec![vec![WALLS_ALL; width]; height],
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
            grid: vec![vec![0; width]; height],
        }
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
        self.grid[y][x]
    }

    fn cell_mut(&mut self, x: usize, y: usize) -> &mut u8 {
        &mut self.grid[y][x]
    }

    /// Whether the given coordinates fall inside the maze.
    pub fn in_bounds(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }

    /// Whether a wall stands on the given side of a cell.
    ///
    /// Asking about a side that leaves the maze always reports a wall: the
    /// outer border can never be removed.
    pub fn has_wall(&self, x: usize, y: usize, direction: Direction) -> bool {
        if x >= self.width || y >= self.height {
            return true;
        }
        let (dx, dy) = direction.offset();
        if !self.in_bounds(x as i64 + dx, y as i64 + dy) {
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
    pub fn neighbors(&self, x: usize, y: usize) -> Vec<(usize, usize, Direction)> {
        let mut result = Vec::with_capacity(4);
        if x >= self.width || y >= self.height {
            return result;
        }
        for direction in Direction::ALL {
            if let Some(neighbor) = self.neighbor(x, y, direction) {
                result.push((neighbor.0, neighbor.1, direction));
            }
        }
        result
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

    /// The set of cells reachable from the top-left corner by walking through
    /// removed walls.
    fn reachable_from_origin(&self) -> HashSet<(usize, usize)> {
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        seen.insert((0, 0));
        queue.push_back((0, 0));
        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny, direction) in self.neighbors(x, y) {
                if self.has_wall(x, y, direction) {
                    continue;
                }
                if seen.insert((nx, ny)) {
                    queue.push_back((nx, ny));
                }
            }
        }
        seen
    }

    /// Whether the bottom-right corner can be reached from the top-left corner.
    pub fn is_solvable(&self) -> bool {
        if self.is_empty() {
            return false;
        }
        self.reachable_from_origin()
            .contains(&(self.width - 1, self.height - 1))
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
        self.openings() == cells - 1 && self.reachable_from_origin().len() == cells
    }

    /// Renders the horizontal wall line above row `y`.
    ///
    /// Row `0` is the top border and row `height` the bottom border; both are
    /// always solid.
    fn horizontal_line(&self, y: usize) -> String {
        let mut line = String::with_capacity(CELL_WIDTH * self.width + 1);
        line.push('+');
        for x in 0..self.width {
            let solid = y == 0 || y == self.height || self.cell(x, y - 1) & WALL_SOUTH != 0;
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
    fn vertical_line(&self, y: usize) -> String {
        let mut line = String::with_capacity(CELL_WIDTH * self.width + 1);
        line.push('|');
        for x in 0..self.width {
            for _ in 0..CELL_WIDTH - 1 {
                line.push(' ');
            }
            let border = x + 1 == self.width;
            let wall = self.cell(x, y) & WALL_EAST != 0;
            line.push(if border || wall { '|' } else { ' ' });
        }
        line
    }

    /// Renders the maze as ASCII art built from `+`, `-` and `|`.
    pub fn to_ascii(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for y in 0..self.height {
            out.push_str(&self.horizontal_line(y));
            out.push('\n');
            out.push_str(&self.vertical_line(y));
            out.push('\n');
        }
        out.push_str(&self.horizontal_line(self.height));
        out
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
        if version != FORMAT_VERSION {
            return Err(invalid(format!(
                "unsupported format version {version}, expected {FORMAT_VERSION}"
            )));
        }

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
                        if a != '-' {
                            return Err(invalid(format!(
                                "art line {row}: the outer border must be solid"
                            )));
                        }
                        if y == 0 {
                            *map.cell_mut(x, 0) |= WALL_NORTH;
                        } else {
                            *map.cell_mut(x, height - 1) |= WALL_SOUTH;
                        }
                    } else {
                        if a == '-' {
                            *map.cell_mut(x, y - 1) |= WALL_SOUTH;
                            *map.cell_mut(x, y) |= WALL_NORTH;
                        }
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
                        if ch != '|' {
                            return Err(invalid(format!(
                                "art line {row}: the outer border must be solid"
                            )));
                        }
                        if x == 0 {
                            *map.cell_mut(0, y) |= WALL_WEST;
                        } else {
                            *map.cell_mut(width - 1, y) |= WALL_EAST;
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
        writeln!(writer, "{MAGIC} {FORMAT_VERSION}")?;
        writeln!(
            writer,
            "{width} {height}",
            width = self.width,
            height = self.height
        )?;
        writeln!(writer, "{ascii}", ascii = self.to_ascii())?;
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

    #[test]
    fn saved_file_starts_with_the_header() {
        let map = Map::new(3, 2);
        let path = temp_path("header.maze");
        map.save_to_file(&path).expect("save failed");
        let text = fs::read_to_string(&path).expect("read failed");
        let _ = fs::remove_file(&path);

        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("maze 1"));
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
}
