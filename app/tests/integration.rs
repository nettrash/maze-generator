//! End-to-end tests that exercise the crate the way a consumer would, through
//! the public API only.
//!
//! Every maze here is built from a seed that the assertion messages name, so
//! a failure is the same maze on every run and can be rebuilt to look at,
//! rather than a one-off the next run never shows again. The tests that touch
//! the disk each work in a `ScratchDir` of their own.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use maze::{ALGORITHMS, AlgorithmType, Direction, Generator, Map, MazeError, MazeOptions, Rng};

/// A directory of one test's own in the system temp directory, removed again
/// when the guard is dropped.
///
/// The tests used to share fixed names such as `maze-integration-svg`, so two
/// `cargo test` runs on one machine wrote into the same directories and one
/// run's cleanup deleted files the other was about to read: with two suites
/// side by side, 42 runs in 200 failed. The name now carries the process id,
/// which keeps concurrent runs apart, a counter, which keeps the tests of one
/// run apart, and the test's name, which says whose a leftover is.
///
/// The cleanup lives in `Drop` rather than at the end of each test so that a
/// failed assertion, which unwinds out of the test, still removes the
/// directory.
struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    fn new(test: &str) -> Self {
        static CREATED: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "maze-integration-{}-{}-{test}",
            std::process::id(),
            CREATED.fetch_add(1, Ordering::Relaxed)
        ));
        // Process ids are reused, and a run killed before its guards ran
        // leaves its directory behind. Starting from that one would mix its
        // files into this test's.
        match fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => panic!("could not clear the stale {}: {error}", path.display()),
        }
        fs::create_dir(&path)
            .unwrap_or_else(|error| panic!("could not create {}: {error}", path.display()));
        ScratchDir { path }
    }

    /// The path of a file inside the directory.
    fn join(&self, file: impl AsRef<Path>) -> PathBuf {
        self.path.join(file)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        // Best effort: panicking here while a failed assertion is already
        // unwinding would abort the whole test binary and bury that failure.
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn every_algorithm_builds_a_perfect_solvable_maze() {
    let seed = 7;
    for name in ALGORITHMS {
        let generator = Generator::new(11, 7, name, Some(seed)).expect("known algorithm");
        let map = generator.build();
        assert_eq!(map.width(), 11, "{name} width, seed {seed}");
        assert_eq!(map.height(), 7, "{name} height, seed {seed}");
        assert!(
            map.is_perfect(),
            "{name} produced an imperfect maze from seed {seed}"
        );
        assert!(
            map.is_solvable(),
            "{name} produced an unsolvable maze from seed {seed}"
        );
    }
}

#[test]
fn generated_mazes_survive_a_save_and_load_round_trip() {
    let dir = ScratchDir::new("generated_mazes_survive_a_save_and_load_round_trip");
    let seed = 13;

    for name in ALGORITHMS {
        let path = dir.join(format!("{name}.maze"));
        let original = Generator::new(9, 6, name, Some(seed))
            .expect("known algorithm")
            .build();
        original.save_to_file(&path).unwrap_or_else(|error| {
            panic!("{name} from seed {seed} could not be saved to {path:?}: {error}")
        });

        let loaded = Map::load_from_file(&path).unwrap_or_else(|error| {
            panic!("{name} from seed {seed} could not be read back: {error}")
        });
        assert_eq!(
            original.to_ascii(),
            loaded.to_ascii(),
            "{name} from seed {seed} did not round-trip through {path:?}"
        );
        assert!(
            loaded.is_perfect(),
            "{name} from seed {seed} loaded an imperfect maze"
        );
    }
}

#[test]
fn map_api_is_usable_directly_from_the_library() {
    let mut map = Map::new(3, 1);
    assert!(!map.is_perfect(), "a fully walled maze is not perfect");

    map.carve(0, 0, Direction::East);
    map.carve(1, 0, Direction::East);
    assert!(map.is_perfect());
    assert!(map.is_solvable());

    // The outer border can never be removed.
    assert!(!map.carve(0, 0, Direction::West));
    assert!(map.has_wall(0, 0, Direction::West));
}

#[test]
fn rng_is_reproducible_from_a_seed() {
    let draw = |seed: u64| {
        let mut rng = Rng::new(seed);
        (0..20).map(|_| rng.below(1000)).collect::<Vec<u64>>()
    };
    assert_eq!(draw(42), draw(42));
    assert_ne!(draw(42), draw(43));
}

#[test]
fn every_algorithm_solves_its_own_maze() {
    let seed = 5;
    for name in ALGORITHMS {
        let generator = Generator::new(13, 8, name, Some(seed)).expect("known algorithm");
        let map = generator.build();

        let route = map
            .solve()
            .unwrap_or_else(|| panic!("{name} produced no route from seed {seed}"));
        assert_eq!(
            route.first(),
            Some(&(0, 0)),
            "{name} route start, seed {seed}"
        );
        assert_eq!(
            route.last(),
            Some(&(12, 7)),
            "{name} route goal, seed {seed}"
        );

        // The route must be a real walk: consecutive cells are neighbours and
        // the wall between them is gone.
        for pair in route.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let direction = match (to.0 as i64 - from.0 as i64, to.1 as i64 - from.1 as i64) {
                (-1, 0) => Direction::West,
                (1, 0) => Direction::East,
                (0, -1) => Direction::North,
                (0, 1) => Direction::South,
                _ => panic!("{name}, seed {seed}: {from:?} and {to:?} are not neighbours"),
            };
            assert!(
                !map.has_wall(from.0, from.1, direction),
                "{name}, seed {seed}: route crosses a wall between {from:?} and {to:?}"
            );
        }

        // A perfect maze has exactly one route, so it cannot revisit a cell.
        let unique: std::collections::HashSet<_> = route.iter().copied().collect();
        assert_eq!(
            unique.len(),
            route.len(),
            "{name} route contains a cycle, seed {seed}"
        );
    }
}

#[test]
fn a_maze_with_a_sealed_exit_is_reported_as_unsolvable() {
    let mut map = Map::new(3, 3);
    map.carve(0, 0, Direction::East);
    map.carve(1, 0, Direction::South);
    assert!(!map.is_solvable());
    assert!(map.solve().is_none());
    // Rendering an unsolved maze must still work so the user can see why.
    assert_eq!(map.to_ascii_with_path(&[]), map.to_ascii());
}

#[test]
fn an_unknown_algorithm_reports_the_valid_choices() {
    let error = Generator::new(4, 4, "depth-first", None)
        .expect_err("an unknown algorithm must be rejected");
    assert!(matches!(error, MazeError::UnsupportedAlgorithm { .. }));

    let message = error.to_string();
    assert!(message.contains("depth-first"), "{message}");
    for name in ALGORITHMS {
        assert!(message.contains(name), "{message} should mention {name}");
    }
}

#[test]
fn algorithm_type_names_match_the_cli_spellings() {
    let cases = [
        (AlgorithmType::Backtracking, "backtracking"),
        (AlgorithmType::Prim, "prim"),
        (AlgorithmType::Kruskal, "kruskal"),
        (AlgorithmType::AldousBroder, "aldous_broder"),
        (AlgorithmType::Wilson, "wilson"),
        (AlgorithmType::Eller, "eller"),
    ];
    for (algorithm, name) in cases {
        assert_eq!(algorithm.as_string(), name);
        assert!(ALGORITHMS.contains(&name), "{name} is not dispatchable");
    }
}

#[test]
fn every_algorithm_exports_valid_svg() {
    let seed = 11;
    for name in ALGORITHMS {
        let generator = Generator::new(7, 5, name, Some(seed)).expect("known algorithm");
        let map = generator.build();
        let svg = map.to_svg();

        assert!(
            svg.starts_with("<svg "),
            "{name} produced no SVG root from seed {seed}"
        );
        assert!(
            svg.trim_end().ends_with("</svg>"),
            "{name} left the root open, seed {seed}"
        );
        assert!(
            svg.contains("xmlns=\"http://www.w3.org/2000/svg\""),
            "{name} wrote no SVG namespace, seed {seed}"
        );
        // A plain maze is fully enclosed, so its border is four straight
        // runs of wall around the 140x100 grid.
        for border in ["M0 0H140", "M0 100H140", "M0 0V100", "M140 0V100"] {
            assert!(
                svg.contains(border),
                "{name} lost the border run {border}, seed {seed}"
            );
        }
    }
}

#[test]
fn svg_export_round_trips_to_disk() {
    let dir = ScratchDir::new("svg_export_round_trips_to_disk");
    let seed = 2;

    let map = Generator::new(6, 4, "kruskal", Some(seed))
        .expect("known algorithm")
        .build();

    let path = dir.join("maze.svg");
    map.save_svg_to_file(&path)
        .expect("could not write the SVG");
    assert_eq!(
        fs::read_to_string(&path).expect("could not read the SVG back"),
        map.to_svg(),
        "kruskal from seed {seed} saved a different SVG from the one it renders"
    );

    let route = map
        .solve()
        .unwrap_or_else(|| panic!("kruskal produced no route from seed {seed}"));
    let solved = dir.join("solved.svg");
    map.save_svg_to_file_with_path(&solved, &route)
        .expect("could not write the solved SVG");
    let solved = fs::read_to_string(&solved).expect("could not read the solved SVG back");
    assert_eq!(
        solved.matches("<polyline ").count(),
        1,
        "kruskal from seed {seed} did not draw exactly one route"
    );
    // Adding a path must not change the walls.
    let walls = |svg: &str| {
        svg.lines()
            .find(|line| line.starts_with("<path "))
            .map(str::to_string)
    };
    assert!(
        walls(&solved).is_some(),
        "the walls went missing, seed {seed}"
    );
    assert_eq!(
        walls(&solved),
        walls(&map.to_svg()),
        "drawing the route moved the walls of kruskal from seed {seed}"
    );
}

#[test]
fn braided_mazes_have_no_dead_ends_and_stay_connected() {
    let seed = 21;
    for name in ALGORITHMS {
        let map = Generator::new(14, 10, name, Some(seed))
            .expect("known algorithm")
            .with_options(MazeOptions {
                braid: true,
                entrance_exit: false,
            })
            .build();

        assert_eq!(
            map.dead_ends().len(),
            0,
            "{name} left a cul-de-sac when braided from seed {seed}"
        );
        assert!(
            map.is_solvable(),
            "{name} became unsolvable when braided from seed {seed}"
        );
        assert!(
            !map.is_perfect(),
            "{name} gained no loop when braided from seed {seed}"
        );
    }
}

#[test]
fn entrance_and_exit_openings_survive_a_round_trip() {
    let dir = ScratchDir::new("entrance_and_exit_openings_survive_a_round_trip");
    let seed = 3;

    let map = Generator::new(10, 8, "wilson", Some(seed))
        .expect("known algorithm")
        .with_options(MazeOptions {
            braid: true,
            entrance_exit: true,
        })
        .build();

    assert!(
        map.is_border_open(0, 0, Direction::West),
        "wilson from seed {seed} has no entrance"
    );
    assert!(
        map.is_border_open(9, 7, Direction::East),
        "wilson from seed {seed} has no exit"
    );

    let path = dir.join("variant.maze");
    map.save_to_file(&path).expect("could not save the maze");
    let loaded = Map::load_from_file(&path)
        .unwrap_or_else(|error| panic!("wilson from seed {seed} could not be read back: {error}"));

    assert_eq!(
        loaded.to_ascii(),
        map.to_ascii(),
        "wilson from seed {seed} changed on a round trip"
    );
    assert!(
        loaded.is_border_open(0, 0, Direction::West),
        "wilson from seed {seed} lost its entrance on a round trip"
    );
    assert!(
        loaded.is_border_open(9, 7, Direction::East),
        "wilson from seed {seed} lost its exit on a round trip"
    );
    assert_eq!(
        loaded.dead_ends().len(),
        0,
        "wilson from seed {seed} loaded with a dead end"
    );
    assert!(
        loaded.is_solvable(),
        "wilson from seed {seed} loaded unsolvable"
    );
}

#[test]
fn a_plain_generator_is_the_default() {
    let seed = 1;
    let generator = Generator::new(6, 6, "prim", Some(seed)).expect("known algorithm");
    assert!(generator.options().is_plain());
    let map = generator.build();
    assert!(
        map.is_perfect(),
        "prim from seed {seed} is not perfect by default"
    );
    assert!(
        !map.dead_ends().is_empty(),
        "prim from seed {seed} was braided by default"
    );
    assert!(
        !map.is_border_open(0, 0, Direction::West),
        "prim from seed {seed} has an entrance by default"
    );
}

/// An unseeded run draws its seed from the clock and reports it, and the
/// report is only worth printing if it is the seed the maze was really built
/// from: the command line prints it so that a maze worth keeping can be had
/// again with `--seed`. Replaying it with the same size, algorithm and
/// variants must therefore give back the identical maze, braided or opened
/// mazes included, since braiding draws from a stream derived from the same
/// seed rather than a fresh one.
#[test]
fn the_seed_reported_for_an_unseeded_run_replays_its_maze() {
    let (width, height) = (12, 9);
    for name in ALGORITHMS {
        for (braid, entrance_exit) in [(false, false), (true, false), (false, true), (true, true)] {
            let options = MazeOptions {
                braid,
                entrance_exit,
            };
            let unseeded = Generator::new(width, height, name, None)
                .expect("known algorithm")
                .with_options(options);
            let seed = unseeded.seed();
            let original = unseeded.build();
            assert_eq!(
                unseeded.seed(),
                seed,
                "{name} with {options:?} changed its seed while building"
            );

            let replayed = Generator::new(width, height, name, Some(seed))
                .expect("known algorithm")
                .with_options(options)
                .build();
            assert_eq!(
                replayed, original,
                "{name} with {options:?} did not replay from its reported seed {seed}"
            );
        }
    }
}
