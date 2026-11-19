//! The error type shared by the library and the command line binary.

use std::fmt;

use crate::generator::ALGORITHMS;

/// Everything that can go wrong while generating a maze or moving one between
/// memory and disk.
///
/// Parse and file errors keep their original [`std::io::Error`] (and its
/// [`std::io::ErrorKind`]) because callers routinely branch on the kind, so
/// they arrive here wrapped in [`MazeError::Io`].
#[derive(Debug)]
pub enum MazeError {
    /// The requested algorithm name does not match any known generator.
    UnsupportedAlgorithm {
        /// The name the user asked for.
        name: String,
    },
    /// A maze cannot have no cells, so a dimension of zero is rejected rather
    /// than quietly producing an empty maze that reports itself as generated.
    EmptySize {
        /// The width the user asked for.
        width: u64,
        /// The height the user asked for.
        height: u64,
    },
    /// The maze is bigger than [`crate::generator::MAX_CELLS`], so building it
    /// would be expected to exhaust memory or run for an unreasonable time.
    TooLarge {
        /// The width the user asked for.
        width: u64,
        /// The height the user asked for.
        height: u64,
        /// The largest number of cells a maze may hold.
        limit: u64,
    },
    /// Reading or writing a maze file failed, or its contents were malformed.
    Io(std::io::Error),
}

impl fmt::Display for MazeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MazeError::UnsupportedAlgorithm { name } => write!(
                f,
                "unsupported algorithm '{name}'. Available algorithms: {}",
                ALGORITHMS.join(", ")
            ),
            MazeError::EmptySize { width, height } => write!(
                f,
                "a maze must have a positive width and height, got {width}x{height}"
            ),
            MazeError::TooLarge {
                width,
                height,
                limit,
            } => {
                let cells = *width as u128 * *height as u128;
                write!(
                    f,
                    "a {width}x{height} maze has {cells} cells, which is more than \
                     the {limit} a maze may have"
                )
            }
            MazeError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for MazeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MazeError::Io(error) => Some(error),
            MazeError::UnsupportedAlgorithm { .. } => None,
            MazeError::EmptySize { .. } => None,
            MazeError::TooLarge { .. } => None,
        }
    }
}

impl From<std::io::Error> for MazeError {
    fn from(error: std::io::Error) -> Self {
        MazeError::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::MazeError;
    use crate::generator::ALGORITHMS;
    use std::error::Error as _;

    #[test]
    fn unsupported_algorithm_lists_every_choice() {
        let error = MazeError::UnsupportedAlgorithm {
            name: "dfs".to_string(),
        };
        let message = error.to_string();
        assert!(message.contains("dfs"), "{message}");
        for name in ALGORITHMS {
            assert!(message.contains(name), "{message} should list {name}");
        }
    }

    #[test]
    fn io_errors_are_wrapped_and_keep_their_source() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let error = MazeError::from(io_error);
        assert!(matches!(error, MazeError::Io(_)));
        assert!(error.source().is_some());
        assert_eq!(error.to_string(), "no such file");
    }

    #[test]
    fn the_error_type_is_usable_as_a_boxed_error() {
        fn build(name: &str) -> Result<(), Box<dyn std::error::Error>> {
            Err(MazeError::UnsupportedAlgorithm {
                name: name.to_string(),
            }
            .into())
        }
        let error = build("dfs").unwrap_err();
        assert!(error.to_string().starts_with("unsupported algorithm 'dfs'"));
    }
}
