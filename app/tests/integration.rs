//! End-to-end tests that exercise the crate the way a consumer would, through
//! the public API only.

use maze::{ALGORITHMS, AlgorithmType, Direction, Generator, Map, MazeError, Rng};

#[tokio::test]
async fn every_algorithm_builds_a_perfect_solvable_maze() {
    for name in ALGORITHMS {
        let generator = Generator::new(11, 7, name).expect("known algorithm");
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
        let original = Generator::new(9, 6, name)
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
    let a: Vec<u64> = (0..20).map(|_| Rng::new(42).below(1000)).collect();
    let b: Vec<u64> = (0..20).map(|_| Rng::new(42).below(1000)).collect();
    assert_eq!(a, b);
}

#[test]
fn an_unknown_algorithm_reports_the_valid_choices() {
    let error = Generator::new(4, 4, "depth-first")
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
