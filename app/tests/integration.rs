//! End-to-end tests that exercise the crate the way a consumer would, through
//! the public API only.

use maze::{ALGORITHMS, AlgorithmType, Direction, Generator, Map, MazeError, Rng};

#[tokio::test]
async fn every_algorithm_builds_a_perfect_solvable_maze() {
    for name in ALGORITHMS {
        let generator = Generator::new(11, 7, name, None).expect("known algorithm");
        let map = generator.generate().await.expect("generation failed");
        assert_eq!(map.width(), 11, "{name} width");
        assert_eq!(map.height(), 7, "{name} height");
        assert!(map.is_perfect(), "{name} produced an imperfect maze");
        assert!(map.is_solvable(), "{name} produced an unsolvable maze");
    }
}

#[tokio::test]
async fn generated_mazes_survive_a_save_and_load_round_trip() {
    let dir = std::env::temp_dir().join("maze-integration-round-trip");
    std::fs::create_dir_all(&dir).unwrap();

    for name in ALGORITHMS {
        let path = dir.join(format!("{name}.maze"));
        let original = Generator::new(9, 6, name, None)
            .expect("known algorithm")
            .generate()
            .await
            .unwrap();
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

#[tokio::test]
async fn every_algorithm_solves_its_own_maze() {
    for name in ALGORITHMS {
        let generator = Generator::new(13, 8, name, Some(5)).expect("known algorithm");
        let map = generator.generate().await.expect("generation failed");

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

#[tokio::test]
async fn a_maze_with_a_sealed_exit_is_reported_as_unsolvable() {
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
