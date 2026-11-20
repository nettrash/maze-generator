//! Maze generation and manipulation.
//!
//! The crate is split into a library (this file) and a thin command line
//! binary. The interesting parts are:
//!
//! - [`Map`] — the in-memory maze, its wall queries, its validation predicates,
//!   its solver, its text file format and its SVG export.
//! - [`algorithm`] — the [`AlgorithmAbstraction`] trait and the six concrete
//!   generators.
//! - [`Generator`] — picks an algorithm by name and runs it.
//! - [`Rng`] — the small deterministic generator the algorithms use.
//!
//! ```
//! use maze::Generator;
//!
//! let generator = Generator::new(8, 6, "backtracking", None).unwrap();
//! let map = generator.build();
//! assert!(map.is_perfect());
//! // A perfect maze has exactly one route from the entrance to the exit.
//! let route = map.solve().expect("a perfect maze is solvable");
//! assert_eq!(route.first(), Some(&(0, 0)));
//! ```

#![warn(missing_docs)]

pub mod algorithm;
pub mod error;
pub mod generator;
pub mod map;
pub mod rng;

pub use algorithm::abstraction::AlgorithmAbstraction;
pub use algorithm::types::AlgorithmType;
pub use error::MazeError;
pub use generator::{ALGORITHMS, Generator, MazeOptions, check_size};
pub use map::{Direction, Map};
pub use rng::{Rng, derive_seed, random_seed};
