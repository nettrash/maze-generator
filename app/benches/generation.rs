//! Benchmarks for maze generation and for the `Map` hot paths.
//!
//! Run them with `cargo bench`. They are excluded from `cargo test`, so the CI
//! test job does not pay for them. Reports land in
//! `target/criterion/report/index.html`, and each named benchmark gets its own
//! directory under `target/criterion/`.
//!
//! Two things are worth watching here. Aldous-Broder and Wilson are
//! asymptotically slower than the four spanning-tree algorithms: they revisit
//! cells rather than carving each wall exactly once, so their cost grows
//! faster than the number of cells. And the `Map` operations below are
//! allocation-light by design, so a change that adds a `Vec` per cell shows up
//! immediately.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use maze::{ALGORITHMS, Generator, Map, MazeOptions, Rng};
use std::fs;
use std::hint::black_box;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Sizes a maze is expected to be generated at: a small one for a page, and a
/// large one for printing.
const SIZES: [(u64, u64); 2] = [(50, 50), (100, 100)];

/// A fixed seed, so every measurement of a benchmark measures the same maze
/// and results stay comparable between runs.
const SEED: u64 = 0x5EED_1234_ABCD_0000;

/// Builds a generator for `algorithm` at the given size.
fn generator(width: u64, height: u64, algorithm: &str) -> Generator {
    Generator::new(width, height, algorithm, Some(SEED)).expect("name came from ALGORITHMS")
}

/// Generation cost per algorithm, at each size.
///
/// The algorithms are benched by name rather than lumped together so that a
/// regression in a single one is obvious in the report.
fn generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("generation");
    for (width, height) in SIZES {
        for algorithm in ALGORITHMS {
            group.bench_with_input(
                BenchmarkId::new(algorithm, format!("{width}x{height}")),
                &(width, height),
                |b, &(width, height)| {
                    let generator = generator(width, height, algorithm);
                    b.iter(|| black_box(generator.build()));
                },
            );
        }
    }
    group.finish();
}

/// Shapes far from square, which is where cost that grows with one side
/// rather than with the number of cells shows.
///
/// Eller used to be quadratic in both directions, so 10x80000 took 1.7 s
/// where it now takes milliseconds; a regression there shows up here first.
/// The random walks get a one-cell-wide corridor, which they carve without
/// walking because a walk along it takes about the square of its length.
fn shapes(c: &mut Criterion) {
    let mut group = c.benchmark_group("shapes");
    group.sample_size(20);
    let cases: [(&str, u64, u64); 4] = [
        ("eller", 10, 100_000),
        ("eller", 100_000, 10),
        ("aldous_broder", 1, 1_000_000),
        ("wilson", 1, 1_000_000),
    ];
    for (algorithm, width, height) in cases {
        group.bench_with_input(
            BenchmarkId::new(algorithm, format!("{width}x{height}")),
            &(width, height),
            |b, &(width, height)| {
                let generator = generator(width, height, algorithm);
                b.iter(|| black_box(generator.build()));
            },
        );
    }
    group.finish();
}

/// How much the variants cost on top of plain generation.
///
/// Braiding walks the whole grid looking for dead ends and then carves, so it
/// should read as a constant extra pass rather than a change of shape.
fn variants(c: &mut Criterion) {
    let mut group = c.benchmark_group("variants");
    let size = (100u64, 100u64);

    let cases = [
        (
            "plain",
            MazeOptions {
                braid: false,
                entrance_exit: false,
            },
        ),
        (
            "braided",
            MazeOptions {
                braid: true,
                entrance_exit: false,
            },
        ),
        (
            "entrance and exit",
            MazeOptions {
                braid: false,
                entrance_exit: true,
            },
        ),
        (
            "both",
            MazeOptions {
                braid: true,
                entrance_exit: true,
            },
        ),
    ];

    for (name, options) in cases {
        group.bench_with_input(BenchmarkId::new("prim", name), &size, |b, &(w, h)| {
            let generator = generator(w, h, "prim").with_options(options);
            b.iter(|| black_box(generator.build()));
        });
    }
    group.finish();
}

/// The `Map` hot paths, measured on a maze that already exists so that
/// generation cost does not drown them out.
fn map_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("map");
    let (width, height) = (100u64, 100u64);
    let map = generator(width, height, "prim").build();

    group.bench_function("to_ascii", |b| {
        b.iter(|| black_box(map.to_ascii()).len());
    });

    // from_ascii reads the whole saved document, not the bare rendering, so
    // the input has to be what save_to_file would have written.
    let text = map.to_text();
    group.bench_function("from_ascii", |b| {
        b.iter(|| {
            let map = black_box(Map::from_ascii(&text)).expect("the text came from to_text");
            black_box(map.width())
        });
    });

    group.bench_function("is_perfect", |b| {
        b.iter(|| black_box(map.is_perfect()));
    });

    group.bench_function("is_solvable", |b| {
        b.iter(|| black_box(map.is_solvable()));
    });

    group.bench_function("solve", |b| {
        b.iter(|| black_box(map.solve()).map(|route| route.len()));
    });

    group.bench_function("dead_ends", |b| {
        b.iter(|| black_box(map.dead_ends()).len());
    });

    // What `check` actually asks for: the number, without the list.
    group.bench_function("dead_end_count", |b| {
        b.iter(|| black_box(map.dead_end_count()));
    });

    // The whole-grid walk is what the algorithms themselves do, so it is the
    // version worth watching for a stray allocation.
    group.bench_function("neighbors over every cell", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for y in 0..height as usize {
                for x in 0..width as usize {
                    total += map.neighbors(x, y).iter().filter(|n| n.is_some()).count();
                }
            }
            black_box(total)
        });
    });

    // Counting neighbours looks at the position only, never at the walls,
    // which is the cheaper path and must stay that way.
    group.bench_function("neighbor_count over every cell", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for y in 0..height as usize {
                for x in 0..width as usize {
                    total += map.neighbor_count(x, y);
                }
            }
            black_box(total)
        });
    });

    group.bench_function("each_neighbor over every cell", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for y in 0..height as usize {
                for x in 0..width as usize {
                    total += map.each_neighbor(x, y).count();
                }
            }
            black_box(total)
        });
    });

    group.bench_function("to_svg", |b| {
        b.iter(|| black_box(map.to_svg()).len());
    });

    group.finish();
}

/// A directory of this benchmark run's own in the system temp directory,
/// removed again when the guard is dropped.
///
/// The name carries the process id because a fixed one was shared by every
/// run on the machine, so one run could delete the file another was timing.
/// The removal lives in `Drop` so that it also happens when a benchmark
/// panics on a failed save or load.
struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("maze-bench-{name}-{}", std::process::id()));
        // Process ids are reused, and a run killed before it could clean up
        // leaves its directory behind under an id this one may now have.
        match fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => panic!("could not clear the stale {}: {error}", path.display()),
        }
        fs::create_dir(&path).expect("could not create the benchmark directory");
        ScratchDir { path }
    }

    /// The path of a file inside the directory.
    fn join(&self, file: impl AsRef<Path>) -> PathBuf {
        self.path.join(file)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        // Best effort: a second panic while one is already unwinding would
        // abort the run and hide the first.
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Saving and loading, which is the only path that touches the disk.
///
/// The directory is made once and reused; each iteration overwrites the same
/// file, so the measurement is the write itself rather than file creation.
fn file_io(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_io");
    let dir = ScratchDir::new("file-io");

    let map = generator(100, 100, "prim").build();
    let path = dir.join("bench.maze");
    let svg_path = dir.join("bench.svg");

    group.bench_function("save_to_file", |b| {
        b.iter(|| black_box(map.save_to_file(&path)).expect("save failed"));
    });

    // The load is unwrapped like the saves: a load that failed would
    // otherwise be timed as if it had worked, and a missing file is a much
    // faster path than parsing one.
    map.save_to_file(&path).expect("save failed");
    group.bench_function("load_from_file", |b| {
        b.iter(|| {
            let map = black_box(Map::load_from_file(&path)).expect("load failed");
            black_box(map.width())
        });
    });

    group.bench_function("save_svg_to_file", |b| {
        b.iter(|| black_box(map.save_svg_to_file(&svg_path)).expect("save failed"));
    });

    group.finish();
}

/// Raw generator throughput, to separate the random number stream from the
/// maze algorithms that consume it.
fn rng(c: &mut Criterion) {
    let mut group = c.benchmark_group("rng");

    group.bench_function("1000 draws below", |b| {
        b.iter(|| {
            let mut rng = Rng::new(SEED);
            let mut sum = 0u64;
            for _ in 0..1000 {
                sum = sum.wrapping_add(rng.below(1_000_000));
            }
            black_box(sum)
        });
    });

    group.bench_function("shuffle 1000 items", |b| {
        b.iter(|| {
            let mut rng = Rng::new(SEED);
            let mut items: Vec<u64> = (0..1000).collect();
            rng.shuffle(&mut items);
            black_box(items[0])
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    generation,
    shapes,
    variants,
    map_operations,
    file_io,
    rng
);
criterion_main!(benches);
