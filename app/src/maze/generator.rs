use std::io::Error;

pub struct Generator {
    pub width: u64,
    pub height: u64,
    pub algorithm: String,
}

impl Generator {
    pub fn new(width: u64, height: u64, algorithm: &str) -> Self {
        Generator {
            width,
            height,
            algorithm: algorithm.to_string(),
        }
    }

    pub async fn generate(&self) -> Result<(), Error> {
        println!(
            "Generating a {}x{} maze using the '{}' algorithm...",
            self.width, self.height, self.algorithm
        );
        // Maze generation logic would go here
        Ok(())
    }
}
