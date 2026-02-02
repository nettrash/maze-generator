/// Represents the maze map.
pub struct Map {
    /// The width of the maze.
    pub width: u64,
    /// The height of the maze.
    pub height: u64,
    /// The grid of the maze, where each cell can be represented by a u8 value.
    pub grid: Vec<Vec<u8>>,
}

impl Map {
    /// Creates a new maze map with the specified width and height.
    pub fn new(width: u64, height: u64) -> Self {
        Map {
            width,
            height,
            grid: vec![vec![0; width as usize]; height as usize],
        }
    }
}

impl Default for Map {
    /// Provides a default maze map of size 100x100.
    fn default() -> Self {
        Map::new(100, 100)
    }
}
