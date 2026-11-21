//! The generation algorithms as a type, and the names they go by.

use std::fmt;
use std::str::FromStr;

use crate::error::MazeError;

/// Which generation algorithm an [`crate::AlgorithmAbstraction`] implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlgorithmType {
    /// The recursive backtracker, a depth-first walk; see
    /// [`crate::algorithm::backtracking`].
    Backtracking,
    /// Randomised Prim's algorithm, grown from a frontier of walls; see
    /// [`crate::algorithm::prim`].
    Prim,
    /// Randomised Kruskal's algorithm, joining regions wall by wall; see
    /// [`crate::algorithm::kruskal`].
    Kruskal,
    /// The Aldous-Broder random walk; see
    /// [`crate::algorithm::aldous_broder`].
    AldousBroder,
    /// Wilson's loop-erased random walks; see [`crate::algorithm::wilson`].
    Wilson,
    /// Eller's row-at-a-time algorithm; see [`crate::algorithm::eller`].
    Eller,
}

impl AlgorithmType {
    /// Every algorithm, in the order [`crate::ALGORITHMS`] advertises them.
    pub const ALL: [AlgorithmType; 6] = [
        AlgorithmType::Backtracking,
        AlgorithmType::Prim,
        AlgorithmType::Kruskal,
        AlgorithmType::AldousBroder,
        AlgorithmType::Wilson,
        AlgorithmType::Eller,
    ];

    /// The name the algorithm goes by in [`crate::ALGORITHMS`] and on the
    /// command line.
    pub const fn as_str(self) -> &'static str {
        match self {
            AlgorithmType::Backtracking => "backtracking",
            AlgorithmType::Prim => "prim",
            AlgorithmType::Kruskal => "kruskal",
            AlgorithmType::AldousBroder => "aldous_broder",
            AlgorithmType::Wilson => "wilson",
            AlgorithmType::Eller => "eller",
        }
    }

    /// The same name as [`AlgorithmType::as_str`].
    ///
    /// The name is `'static`: it used to borrow from `self`, which tied it to
    /// the temporary that [`crate::AlgorithmAbstraction::get_algorithm_type`]
    /// returns, so it could not be kept in a variable.
    pub fn as_string(&self) -> &'static str {
        self.as_str()
    }
}

impl fmt::Display for AlgorithmType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AlgorithmType {
    type Err = MazeError;

    /// Looks a name up exactly, case and whitespace included, so the names
    /// the tool accepts are the names it advertises.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        AlgorithmType::ALL
            .into_iter()
            .find(|kind| kind.as_str() == name)
            .ok_or_else(|| MazeError::UnsupportedAlgorithm {
                name: name.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::AlgorithmType;

    #[test]
    fn as_string_returns_expected_values() {
        let cases = [
            (AlgorithmType::Backtracking, "backtracking"),
            (AlgorithmType::Prim, "prim"),
            (AlgorithmType::Kruskal, "kruskal"),
            (AlgorithmType::AldousBroder, "aldous_broder"),
            (AlgorithmType::Wilson, "wilson"),
            (AlgorithmType::Eller, "eller"),
        ];

        for (algorithm, expected) in cases {
            assert_eq!(algorithm.as_string(), expected);
            assert_eq!(algorithm.as_str(), expected);
            assert_eq!(algorithm.to_string(), expected);
            assert_eq!(expected.parse::<AlgorithmType>().ok(), Some(algorithm));
        }
    }

    /// `ALL` has to name every variant. The match has no wildcard arm, so a
    /// new variant stops this test compiling until it is handled here, which
    /// is the reminder to add it to `ALL` too.
    #[test]
    fn all_holds_every_variant_once() {
        let mut seen = [false; AlgorithmType::ALL.len()];
        for kind in AlgorithmType::ALL {
            let slot = match kind {
                AlgorithmType::Backtracking => 0,
                AlgorithmType::Prim => 1,
                AlgorithmType::Kruskal => 2,
                AlgorithmType::AldousBroder => 3,
                AlgorithmType::Wilson => 4,
                AlgorithmType::Eller => 5,
            };
            assert!(!seen[slot], "{kind} is listed twice");
            seen[slot] = true;
        }
        assert!(seen.iter().all(|&listed| listed));
    }

    #[test]
    fn names_are_matched_exactly() {
        for bad in ["Backtracking", " prim", "prim ", "primus", ""] {
            assert!(bad.parse::<AlgorithmType>().is_err(), "{bad:?}");
        }
    }
}
