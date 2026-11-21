//! Property-based tests over the generation algorithms and the text format.
//!
//! The unit tests assert a handful of fixed seeds and sizes. These tests
//! sample the input space instead: random sizes, random seeds, every
//! algorithm, checking the invariants that must hold for *every* maze rather
//! than for the few that were written down.
//!
//! Case counts and grid sizes are deliberately bounded so the suite stays
//! fast enough for CI.

use maze::{ALGORITHMS, AlgorithmType, Direction, Generator, Map, MazeOptions, Rng};
use proptest::prelude::*;
use proptest::sample::Index;

/// Enough cases to cover all six algorithms many times over while keeping the
/// whole file well under a second, so CI does not care. Raise it locally when
/// changing an algorithm.
const CASES: u32 = 512;

/// The four directions, so the tests do not repeat the array literal.
const DIRECTIONS: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];

/// Grid sizes used by the property tests. The lower bound of 1 keeps the
/// degenerate shapes in the sample; the upper bound keeps a case cheap.
fn sizes() -> impl Strategy<Value = (u64, u64)> {
    (1u64..=40u64, 1u64..=40u64)
}

/// Picks one of the algorithms, so a failing case shrinks towards the first
/// algorithm in the list.
fn algorithms() -> impl Strategy<Value = &'static str> {
    prop::sample::select(ALGORITHMS.to_vec())
}

/// A maze straight out of a generator, with the variants applied.
#[derive(Debug, Clone)]
struct ArbitraryMaze {
    map: Map,
    options: MazeOptions,
    algorithm: &'static str,
}

fn arbitrary_maze() -> impl Strategy<Value = ArbitraryMaze> {
    (
        sizes(),
        any::<u64>(),
        algorithms(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|((width, height), seed, algorithm, braid, entrance_exit)| {
            let options = MazeOptions {
                braid,
                entrance_exit,
            };
            let map = Generator::new(width, height, algorithm, Some(seed))
                .expect("algorithm came from ALGORITHMS")
                .with_options(options)
                .build();
            ArbitraryMaze {
                map,
                options,
                algorithm,
            }
        })
}

/// Every side of the outer border, each exactly once: the north side of every
/// cell in the top row, the south side of every cell in the bottom row, and
/// the west and east sides of every cell in the first and last columns.
///
/// Listed from the geometry rather than found with [`Map::neighbor`], so the
/// count does not lean on the code it checks. A border side belongs to one
/// cell only, so none is listed twice; a 1x1 maze, whose single cell carries
/// all four, gives four distinct sides.
fn border_sides(width: u64, height: u64) -> impl Iterator<Item = (usize, usize, Direction)> {
    let (last_x, last_y) = (width as usize - 1, height as usize - 1);
    let rows =
        (0..=last_x).flat_map(move |x| [(x, 0, Direction::North), (x, last_y, Direction::South)]);
    let columns =
        (0..=last_y).flat_map(move |y| [(0, y, Direction::West), (last_x, y, Direction::East)]);
    rows.chain(columns)
}

/// How many sides of the outer border are open.
fn open_border_sides(map: &Map) -> usize {
    border_sides(map.width(), map.height())
        .filter(|&(x, y, direction)| map.is_border_open(x, y, direction))
        .count()
}

/// What a single-character edit puts into a saved maze: every character the
/// format draws with, a line break, and one it never uses.
const EDIT_CHARS: [char; 6] = ['+', '-', '|', ' ', '\n', 'x'];

/// One character inserted, deleted or replaced, the smallest damage a saved
/// maze can take, whether from a slip in a hand edit or a corrupted file.
///
/// The position is an [`Index`] rather than a number, so it is scaled to the
/// length of whatever text the edit lands on and every position, the very end
/// included, can be drawn.
#[derive(Debug, Clone, Copy)]
enum Edit {
    Insert(Index, char),
    Delete(Index),
    Replace(Index, char),
}

impl Edit {
    /// The text with the edit applied. Positions are byte offsets, so the
    /// text must be ASCII for every one of them to fall on a character.
    fn apply(self, text: &str) -> String {
        let mut edited = text.to_string();
        match self {
            Edit::Insert(at, ch) => edited.insert(at.index(text.len() + 1), ch),
            Edit::Delete(at) => {
                edited.remove(at.index(text.len()));
            }
            Edit::Replace(at, ch) => {
                let at = at.index(text.len());
                edited.replace_range(at..=at, &ch.to_string());
            }
        }
        edited
    }
}

fn edits() -> impl Strategy<Value = Edit> {
    let chars = || prop::sample::select(EDIT_CHARS.to_vec());
    prop_oneof![
        (any::<Index>(), chars()).prop_map(|(at, ch)| Edit::Insert(at, ch)),
        any::<Index>().prop_map(Edit::Delete),
        (any::<Index>(), chars()).prop_map(|(at, ch)| Edit::Replace(at, ch)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// The core promise of the crate: whatever the size, seed or algorithm,
    /// the result is a perfect maze with a route from corner to corner.
    #[test]
    fn every_generated_maze_is_perfect_and_solvable(
        (width, height) in sizes(),
        seed in any::<u64>(),
        algorithm in algorithms(),
    ) {
        let map = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .build();

        prop_assert_eq!(map.width(), width, "{} changed the width", algorithm);
        prop_assert_eq!(map.height(), height, "{} changed the height", algorithm);
        prop_assert!(map.is_solvable(), "{} made an unsolvable maze", algorithm);
        prop_assert!(map.is_perfect(), "{} made an imperfect maze", algorithm);
    }

    /// The shapes a hand-written test tends to forget: a single cell, a
    /// single row and a single column.
    #[test]
    fn degenerate_shapes_are_still_perfect(
        longest in 1u64..=60u64,
        seed in any::<u64>(),
        algorithm in algorithms(),
    ) {
        for (width, height) in [(1, 1), (1, longest), (longest, 1)] {
            let map = Generator::new(width, height, algorithm, Some(seed))
                .expect("known algorithm")
                .build();

            prop_assert_eq!(map.width(), width);
            prop_assert_eq!(map.height(), height);
            prop_assert!(
                map.is_solvable(),
                "{} made an unsolvable {}x{} maze", algorithm, width, height
            );
            prop_assert!(
                map.is_perfect(),
                "{} made an imperfect {}x{} maze", algorithm, width, height
            );
        }
    }

    /// A seed is a promise: the same width, height, algorithm and options
    /// must reproduce the identical maze.
    #[test]
    fn generation_is_reproducible_from_its_seed(
        (width, height) in sizes(),
        seed in any::<u64>(),
        algorithm in algorithms(),
        braid in any::<bool>(),
        entrance_exit in any::<bool>(),
    ) {
        let options = MazeOptions { braid, entrance_exit };
        let first = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .with_options(options)
            .build();
        let second = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .with_options(options)
            .build();

        prop_assert_eq!(first, second, "{} is not reproducible from its seed", algorithm);
    }

    /// The seed handed in is the seed reported back, whatever the size,
    /// algorithm or variants, so the seed a run prints is one that replays it.
    ///
    /// Zero and `u64::MAX` are drawn on purpose: zero is the classic value
    /// mistaken for "no seed given", and a uniform draw never lands on it.
    #[test]
    fn a_generator_reports_the_seed_it_was_given(
        (width, height) in sizes(),
        seed in prop_oneof![1 => Just(0u64), 1 => Just(u64::MAX), 8 => any::<u64>()],
        algorithm in algorithms(),
        braid in any::<bool>(),
        entrance_exit in any::<bool>(),
    ) {
        let generator = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .with_options(MazeOptions { braid, entrance_exit });
        prop_assert_eq!(generator.seed(), seed, "{} reported a different seed", algorithm);
    }

    /// Editors and whitespace hooks strip trailing spaces, which every row of
    /// cells ending in an exit has. Only spaces can have been lost, so the
    /// maze must load back exactly; the size check once refused such files as
    /// too short before the spaces could be put back.
    #[test]
    fn a_saved_maze_loads_after_its_trailing_spaces_are_stripped(maze in arbitrary_maze()) {
        let stripped: String = maze
            .map
            .to_text()
            .lines()
            .map(|line| format!("{}\n", line.trim_end()))
            .collect();
        let loaded = Map::from_ascii(&stripped);
        prop_assert!(loaded.is_ok(), "{} stripped: {:?}", maze.algorithm, loaded.err());
        prop_assert_eq!(&loaded.expect("checked above"), &maze.map);
    }

    /// Saving and loading must be exact, including the extra openings the
    /// variants add. `to_text` is what `save_to_file` writes and `from_ascii`
    /// reads it back, so this is the save/load round trip without a disk.
    #[test]
    fn the_text_format_round_trips(maze in arbitrary_maze()) {
        let text = maze.map.to_text();
        let parsed = Map::from_ascii(&text);
        prop_assert!(parsed.is_ok(), "{} wrote unreadable text: {:?}", maze.algorithm, parsed.err());
        let loaded = parsed.expect("checked above");

        prop_assert_eq!(
            &loaded.to_text(),
            &text,
            "{} lost walls on a round trip", maze.algorithm
        );
        prop_assert_eq!(&loaded, &maze.map, "{} changed shape on a round trip", maze.algorithm);

        // The entrance and exit are the part most likely to be dropped, since
        // they live in the border rather than between cells.
        if maze.options.entrance_exit {
            let last_x = maze.map.width() as usize - 1;
            let last_y = maze.map.height() as usize - 1;
            prop_assert!(loaded.is_border_open(0, 0, Direction::West));
            prop_assert!(loaded.is_border_open(last_x, last_y, Direction::East));
        }
    }

    /// Braiding is allowed to break perfection, but never connectivity: a
    /// braided maze must still be solvable, and the only cul-de-sacs left are
    /// the ones braiding cannot reach.
    ///
    /// Braiding only knocks down interior walls, so a dead end survives
    /// exactly when every wall it still has is a border wall. That happens in
    /// very narrow mazes: in a 2x1 corridor both cells have a single opening
    /// between them and nothing but border on their other three sides.
    #[test]
    fn braided_mazes_stay_connected_and_lose_the_reachable_dead_ends(
        (width, height) in sizes(),
        seed in any::<u64>(),
        algorithm in algorithms(),
    ) {
        let map = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .with_options(MazeOptions { braid: true, entrance_exit: false })
            .build();

        prop_assert!(map.is_solvable(), "{} braided itself into an unsolvable maze", algorithm);

        for (x, y) in map.dead_ends() {
            let walled_interior = DIRECTIONS
                .iter()
                .filter(|direction| map.neighbor(x, y, **direction).is_some())
                .filter(|direction| map.has_wall(x, y, **direction))
                .count();
            prop_assert_eq!(
                walled_interior, 0,
                "{} left a braidable dead end at ({}, {}) when braided", algorithm, x, y
            );
        }
    }

    /// Opening the border must not disturb the interior: the maze stays
    /// solvable, and only the two requested holes appear.
    ///
    /// The holes are the west side of the top-left cell and the east side of
    /// the bottom-right one. In a 1x1 maze those are the same cell but still
    /// two different sides, so exactly two border sides are open at every
    /// size. The plain maze has none: the algorithms only ever carve between
    /// two cells, and a side on the border has no cell beyond it.
    #[test]
    fn entrance_and_exit_only_open_the_border(
        (width, height) in sizes(),
        seed in any::<u64>(),
        algorithm in algorithms(),
    ) {
        let plain = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .build();
        let opened = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .with_options(MazeOptions { braid: false, entrance_exit: true })
            .build();

        prop_assert!(opened.is_solvable(), "{} became unsolvable when opened", algorithm);
        prop_assert!(opened.is_border_open(0, 0, Direction::West));
        prop_assert!(opened.is_border_open(width as usize - 1, height as usize - 1, Direction::East));

        // The two holes checked above are the only ones.
        prop_assert_eq!(
            open_border_sides(&plain), 0,
            "{} opened the border of a plain maze", algorithm
        );
        prop_assert_eq!(
            open_border_sides(&opened), 2,
            "{} opened more of the border than the entrance and the exit", algorithm
        );

        // The interior walls are untouched: every wall between two cells is
        // exactly what it was in the plain maze.
        for y in 0..height as usize {
            for x in 0..width as usize {
                for direction in DIRECTIONS {
                    if opened.neighbor(x, y, direction).is_some() {
                        prop_assert_eq!(
                            opened.has_wall(x, y, direction),
                            plain.has_wall(x, y, direction),
                            "{} changed an interior wall while opening the border", algorithm
                        );
                    }
                }
            }
        }
    }

    /// The solver returns a route that really walks the maze.
    #[test]
    fn a_solution_is_a_walkable_route(
        (width, height) in sizes(),
        seed in any::<u64>(),
        algorithm in algorithms(),
        braid in any::<bool>(),
    ) {
        let map = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .with_options(MazeOptions { braid, entrance_exit: false })
            .build();

        let route = map.solve().expect("a generated maze is always solvable");
        let exit = (width as usize - 1, height as usize - 1);
        prop_assert_eq!(route.first(), Some(&(0, 0)), "the route does not start at the entrance");
        prop_assert_eq!(route.last(), Some(&exit), "the route does not end at the exit");

        // Every step moves to an orthogonally adjacent cell with no wall between.
        for pair in route.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let dx = to.0 as i64 - from.0 as i64;
            let dy = to.1 as i64 - from.1 as i64;
            prop_assert!((dx.abs() + dy.abs()) == 1, "the route jumps from {:?} to {:?}", from, to);

            let direction = match (dx, dy) {
                (0, -1) => Direction::North,
                (1, 0) => Direction::East,
                (0, 1) => Direction::South,
                _ => Direction::West,
            };
            prop_assert!(
                !map.has_wall(from.0, from.1, direction),
                "the route walks through a wall from {:?} to {:?}", from, to
            );
        }
    }

    /// Braiding only ever removes walls, so it can never build one back where
    /// the plain maze had none.
    #[test]
    fn braiding_only_removes_walls(
        width in 2u64..=30u64,
        height in 2u64..=30u64,
        seed in any::<u64>(),
        algorithm in algorithms(),
    ) {
        let plain = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .build();
        let braided = Generator::new(width, height, algorithm, Some(seed))
            .expect("known algorithm")
            .with_options(MazeOptions { braid: true, entrance_exit: false })
            .build();

        for y in 0..height as usize {
            for x in 0..width as usize {
                for direction in DIRECTIONS {
                    if !plain.has_wall(x, y, direction) {
                        prop_assert!(
                            !braided.has_wall(x, y, direction),
                            "{} built a wall back at ({}, {}) {:?} when braiding",
                            algorithm, x, y, direction
                        );
                    }
                }
            }
        }
    }

    /// The SVG export is a self-contained document for every size and variant.
    #[test]
    fn svg_export_is_a_complete_document(maze in arbitrary_maze()) {
        let svg = maze.map.to_svg();
        prop_assert!(svg.starts_with("<svg "), "{} wrote something that is not SVG", maze.algorithm);
        prop_assert!(svg.trim_end().ends_with("</svg>"), "{} left the SVG root open", maze.algorithm);
        prop_assert!(svg.contains("viewBox=\"-1 -1 "), "{} wrote a viewBox-less SVG", maze.algorithm);
    }

    /// The parser must reject or accept, never panic, however mangled the
    /// input is.
    ///
    /// The `(?s)` lets `.` match a line break as well. Without it every input
    /// was a single line, which no maze file is.
    #[test]
    fn the_parser_never_panics_on_garbage(text in "(?s).{0,400}") {
        let _ = Map::from_ascii(&text);
    }

    /// The same, for a well-formed header claiming any size at all. Random
    /// text almost never makes a header, which is how a two-line file that
    /// aborted the process slipped past the test above: the parser allocated
    /// whatever grid the header asked for before reading any art.
    #[test]
    fn the_parser_never_panics_whatever_size_the_header_claims(
        version in 1u32..=2,
        width in prop_oneof![0u64..8, any::<u64>()],
        height in prop_oneof![0u64..8, any::<u64>()],
        art in "[-+| \n]{0,120}",
    ) {
        let _ = Map::from_ascii(&format!("maze {version}\n{width} {height}\n{art}"));
    }

    /// The header varied as well as the size: the marker or a near miss and
    /// versions on both sides of the supported range, with sizes at the edges
    /// of the checks, from zero through a width of 2^32, which a 64-bit parser
    /// turns into art far longer than the input, to `u64::MAX`, whose art
    /// length overflows. The art is drawn from the format's own characters,
    /// line breaks included.
    #[test]
    fn the_parser_never_panics_on_a_plausible_header(
        marker in prop_oneof![Just("maze".to_string()), "[a-z]{0,5}"],
        version in 0u32..=3,
        width in prop_oneof![0u64..=6, Just(u64::MAX), Just(1u64 << 32)],
        height in prop_oneof![0u64..=6, Just(u64::MAX)],
        art in "(?s)[-+| \n]{0,200}",
    ) {
        let _ = Map::from_ascii(&format!("{marker} {version}\n{width} {height}\n{art}"));
    }

    /// A saved maze with a single character inserted, deleted or replaced is
    /// as close to valid as damage gets, so it reaches the deepest parts of
    /// the parser. It must be refused or read, never panic, and whatever is
    /// read must be a maze in its own right: one that writes out and reads
    /// back as itself, like any other.
    #[test]
    fn a_saved_maze_with_one_character_edited_is_refused_or_round_trips(
        maze in arbitrary_maze(),
        edit in edits(),
    ) {
        let text = maze.map.to_text();
        prop_assert!(text.is_ascii(), "{} wrote non-ASCII text", maze.algorithm);
        let edited = edit.apply(&text);

        if let Ok(loaded) = Map::from_ascii(&edited) {
            let reloaded = Map::from_ascii(&loaded.to_text());
            prop_assert!(
                reloaded.is_ok(),
                "{} after {:?} was read, but its own text was not: {:?}",
                maze.algorithm, edit, reloaded.err()
            );
            prop_assert_eq!(
                reloaded.expect("checked above"), loaded,
                "{} after {:?} changed on a second round trip", maze.algorithm, edit
            );
        }
    }

    /// Seeding the random generator is the difference between a demo that can
    /// be reproduced and one that cannot.
    #[test]
    fn a_seeded_rng_repeats_and_an_unseeded_one_does_not(seed in any::<u64>()) {
        fn draw(rng: &mut Rng) -> Vec<u64> {
            let mut values = Vec::with_capacity(32);
            for _ in 0..16 {
                values.push(rng.below(1 << 40));
                values.push(rng.coin() as u64);
            }
            values
        }

        let first = draw(&mut Rng::new(seed));
        let second = draw(&mut Rng::new(seed));
        prop_assert_eq!(first, second, "Rng::new(seed) is not deterministic");

        let unseeded_a = draw(&mut Rng::from_entropy());
        let unseeded_b = draw(&mut Rng::from_entropy());
        prop_assert_ne!(unseeded_a, unseeded_b, "two entropy-seeded generators agreed");
    }
}

/// The variant flags must be visible on the generator, so a caller can tell
/// what a `Generator` will do before asking it to build.
#[test]
fn options_round_trip_through_the_generator() {
    for (braid, entrance_exit) in [(false, false), (true, false), (false, true), (true, true)] {
        let options = MazeOptions {
            braid,
            entrance_exit,
        };
        let generator = Generator::new(6, 6, "prim", Some(1))
            .expect("known algorithm")
            .with_options(options);

        assert_eq!(generator.options(), options);
        assert_eq!(generator.options().is_plain(), !braid && !entrance_exit);
    }
}

/// The CLI's list of names and the `AlgorithmType` enum must stay in step, or
/// a documented algorithm quietly stops working.
#[test]
fn the_algorithm_list_and_the_enum_agree() {
    let kinds = [
        AlgorithmType::Backtracking,
        AlgorithmType::Prim,
        AlgorithmType::Kruskal,
        AlgorithmType::AldousBroder,
        AlgorithmType::Wilson,
        AlgorithmType::Eller,
    ];

    assert_eq!(kinds.len(), ALGORITHMS.len());
    for kind in kinds.iter() {
        let name = kind.as_string();
        assert!(ALGORITHMS.contains(&name), "{name} is not dispatchable");
    }
    for name in ALGORITHMS {
        assert!(
            kinds.iter().any(|kind| kind.as_string() == name),
            "{name} is dispatchable but has no AlgorithmType"
        );
    }
}
