# Maze Generator

Just for fun. This tool can generate the random maze by different algorythms.
Each maze has only one path from left top corner to the right bottom corner.

## Install

Build the binary from the `app` directory:

```sh
cd app && cargo build --release   # target/release/maze
```

Or pull the published image, which is built for `linux/amd64` and
`linux/arm64`:

```sh
docker run --rm nettrash/maze:0.0.1 maze --command generate --width 20 --height 10
```

## Usage

```sh
# Print a maze to the terminal
maze --command generate --width 20 --height 10 --algorithm backtracking

# Save a maze to a file
maze --command generate --width 20 --height 10 --algorithm eller --output maze.txt

# Verify a saved maze
maze --command check --input maze.txt

# Print a maze with its solution path drawn on it
maze --command solve --input maze.txt

# Export a maze as a vector drawing, for printing or sharing
maze --command generate --width 30 --height 20 --output maze.svg
maze --command solve --input maze.txt --output solved.svg

# Reproduce a maze you liked
maze --command generate --width 20 --height 10 --algorithm wilson --seed 42
```

Available algorithms: `backtracking`, `prim`, `kruskal`, `aldous_broder`,
`wilson`, `eller`. All of them produce a perfect maze, meaning exactly one path
connects any two cells.

## Solving a maze

Because a perfect maze has exactly one route between two cells, `solve` prints
that route rather than a choice of routes. The entrance is marked `S`, the exit
`E`, and the corridors in between with `.`. The number of steps is reported
afterwards. A maze whose exit has been sealed off is printed unsolved and the
command exits with a failure status.

## Output formats

`--output` writes the plain text format by default, which is the only format
`--input` can read back. A `.svg` name writes a vector drawing instead, and
`--format text|svg` overrides that choice whatever the file is called. SVG is
for printing and sharing: walls are strokes on a white background, so the file
stays sharp at any size, but it is a one-way export. `solve --output` draws the
solution path as a green line under the walls.

## Maze variants

By default every maze is *perfect*: the walls form a spanning tree, so there is
exactly one route between any two cells. Two optional shapes change that.

**Braided** mazes have their dead ends knocked out with extra openings, so
there are loops and more than one way around:

```sh
maze --command generate --width 20 --height 20 --algorithm prim --braid
```

**Entrance and exit** openings punch a hole in the outer border at the start and
end cells, so the maze reads as somewhere you can actually walk in:

```sh
maze --command generate --width 20 --height 20 --algorithm prim --entrance-exit
```

The flags combine, and both survive a save/load round trip. A braided maze is no
longer perfect, so `check` reports `Perfect: false` and `Dead ends: 0` for it,
and `solve` returns the shortest of the several routes available.

## Reproducible mazes

Every run picks a random seed and prints it, so passing `--seed` is optional.
The same seed with the same width, height and algorithm always produces the
same maze, which makes a maze worth keeping easy to regenerate.

## Maze file format

A saved maze is a plain text document: a `maze 2` marker line, a `width height`
line, and the ASCII rendering of the maze. The rendering is lossless, so loading
a saved maze reproduces the maze it came from, dead-end openings and border holes
included. Version 1 files are still readable, but one with a hole in its outer
border is rejected, since version 1 could not represent that.

## Testing

```sh
cd app
cargo test              # unit, integration and property tests
cargo test --test properties   # the property tests on their own
cargo bench             # benchmarks, and they are not part of cargo test
```

Alongside the hand-written unit and integration tests, `tests/properties.rs`
uses [`proptest`](https://proptest-rs.github.io/proptest/) to check the
invariants that must hold for *every* maze rather than for a few fixed seeds:
every algorithm produces a perfect, solvable maze at any size including `1x1`,
`1xN` and `Nx1`; a seed always reproduces its maze; the text format round
trips exactly; braiding never disconnects a maze nor builds a wall back; and
every route `solve` reports really walks the maze without crossing a wall.

## Benchmarks

`cargo bench` runs the [criterion](https://bheisler.github.io/criterion.rs/)
benchmarks in `app/benches/generation.rs` and writes an HTML report to
`app/target/criterion/report/index.html`. They are a separate target, so the
normal `cargo test` run does not pay for them.

They cover generation for all six algorithms at 50x50 and 100x100, the cost of
the `--braid` and `--entrance-exit` variants on top of plain generation, the
`Map` hot paths (`to_ascii`, `from_ascii`, `is_perfect`, `is_solvable`, `solve`,
`dead_ends`, `dead_end_count`, `neighbors`, `neighbor_count`, `each_neighbor`,
`to_svg`), file saving and loading, and the random number generator on its own.

Expect Aldous-Broder and Wilson to be the slowest by a wide margin: unlike the
four spanning-tree algorithms they revisit cells rather than carving each wall
exactly once, so their cost grows faster than the number of cells. On a 100x100
grid that is roughly 3.5 ms for Aldous-Broder and 0.5 ms for Wilson, against
about 0.1 to 0.3 ms for the rest.
