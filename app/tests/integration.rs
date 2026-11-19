//! End-to-end tests that exercise the crate the way a consumer would, through
//! the public API only.

use maze::{ALGORITHMS, AlgorithmType, Direction, Generator, Map, MazeError, MazeOptions, Rng};

#[test]
fn every_algorithm_builds_a_perfect_solvable_maze() {
    for name in ALGORITHMS {
        let generator = Generator::new(11, 7, name, None).expect("known algorithm");
        let map = generator.build();
        assert_eq!(map.width(), 11, "{name} width");
        assert_eq!(map.height(), 7, "{name} height");
        assert!(map.is_perfect(), "{name} produced an imperfect maze");
        assert!(map.is_solvable(), "{name} produced an unsolvable maze");
    }
}

#[test]
fn generated_mazes_survive_a_save_and_load_round_trip() {
    let dir = std::env::temp_dir().join("maze-integration-round-trip");
    std::fs::create_dir_all(&dir).unwrap();

    for name in ALGORITHMS {
        let path = dir.join(format!("{name}.maze"));
        let original = Generator::new(9, 6, name, None)
            .expect("known algorithm")
            .build();
        original.save_to_file(&path).unwrap();

        let loaded = Map::load_from_file(&path).unwrap();
        assert_eq!(
            original.to_ascii(),
            loaded.to_ascii(),
            "{name} did not round-trip through {path:?}"
        );
        assert!(loaded.is_perfect(), "{name} loaded an imperfect maze");
    }

    std::fs::remove_dir_all(&dir).ok();
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
    for name in ALGORITHMS {
        let generator = Generator::new(13, 8, name, Some(5)).expect("known algorithm");
        let map = generator.build();

        let route = map
            .solve()
            .unwrap_or_else(|| panic!("{name} produced no route"));
        assert_eq!(route.first(), Some(&(0, 0)), "{name} route start");
        assert_eq!(route.last(), Some(&(12, 7)), "{name} route goal");

        // The route must be a real walk: consecutive cells are neighbours and
        // the wall between them is gone.
        for pair in route.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let direction = match (to.0 as i64 - from.0 as i64, to.1 as i64 - from.1 as i64) {
                (-1, 0) => Direction::West,
                (1, 0) => Direction::East,
                (0, -1) => Direction::North,
                (0, 1) => Direction::South,
                _ => panic!("{name}: {from:?} and {to:?} are not neighbours"),
            };
            assert!(
                !map.has_wall(from.0, from.1, direction),
                "{name}: route crosses a wall between {from:?} and {to:?}"
            );
        }

        // A perfect maze has exactly one route, so it cannot revisit a cell.
        let unique: std::collections::HashSet<_> = route.iter().copied().collect();
        assert_eq!(unique.len(), route.len(), "{name} route contains a cycle");
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
        .err()
        .expect("an unknown algorithm must be rejected");
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
    for name in ALGORITHMS {
        let generator = Generator::new(7, 5, name, Some(11)).expect("known algorithm");
        let map = generator.build();
        let svg = map.to_svg();

        assert!(svg.starts_with("<svg "), "{name} produced no SVG root");
        assert!(
            svg.trim_end().ends_with("</svg>"),
            "{name} left the root open"
        );
        assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
        // The document grows with the maze: a 7x5 maze has at least its border.
        assert!(
            svg.matches("<line ").count() >= 2 * (7 + 5),
            "{name} drew too few walls"
        );
    }
}

#[test]
fn svg_export_round_trips_to_disk() {
    let dir = std::env::temp_dir().join("maze-integration-svg");
    std::fs::create_dir_all(&dir).unwrap();

    let map = Generator::new(6, 4, "kruskal", Some(2))
        .expect("known algorithm")
        .build();

    let path = dir.join("maze.svg");
    map.save_svg_to_file(&path).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), map.to_svg());

    let route = map.solve().expect("a perfect maze is solvable");
    let solved = dir.join("solved.svg");
    map.save_svg_to_file_with_path(&solved, &route).unwrap();
    let solved = std::fs::read_to_string(&solved).unwrap();
    assert_eq!(solved.matches("<polyline ").count(), 1);
    // Adding a path must not change the walls.
    assert_eq!(
        solved.matches("<line ").count(),
        map.to_svg().matches("<line ").count()
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn braided_mazes_have_no_dead_ends_and_stay_connected() {
    for name in ALGORITHMS {
        let map = Generator::new(14, 10, name, Some(21))
            .expect("known algorithm")
            .with_options(MazeOptions {
                braid: true,
                entrance_exit: false,
            })
            .build();

        assert_eq!(
            map.dead_ends().len(),
            0,
            "{name} left a cul-de-sac when braided"
        );
        assert!(map.is_solvable(), "{name} became unsolvable when braided");
        assert!(!map.is_perfect(), "{name} gained no loop when braided");
    }
}

#[test]
fn entrance_and_exit_openings_survive_a_round_trip() {
    let dir = std::env::temp_dir().join("maze-integration-variants");
    std::fs::create_dir_all(&dir).unwrap();

    let map = Generator::new(10, 8, "wilson", Some(3))
        .expect("known algorithm")
        .with_options(MazeOptions {
            braid: true,
            entrance_exit: true,
        })
        .build();

    assert!(map.is_border_open(0, 0, Direction::West));
    assert!(map.is_border_open(9, 7, Direction::East));

    let path = dir.join("variant.maze");
    map.save_to_file(&path).unwrap();
    let loaded = Map::load_from_file(&path).unwrap();
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(loaded.to_ascii(), map.to_ascii());
    assert!(loaded.is_border_open(0, 0, Direction::West));
    assert!(loaded.is_border_open(9, 7, Direction::East));
    assert_eq!(loaded.dead_ends().len(), 0);
    assert!(loaded.is_solvable());
}

#[test]
fn a_plain_generator_is_the_default() {
    let generator = Generator::new(6, 6, "prim", Some(1)).expect("known algorithm");
    assert!(generator.options().is_plain());
    let map = generator.build();
    assert!(map.is_perfect());
    assert!(!map.dead_ends().is_empty());
    assert!(!map.is_border_open(0, 0, Direction::West));
}
