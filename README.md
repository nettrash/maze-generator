# Maze Generator

Just for fun. This tool generates a random maze with any of six algorithms.
Each maze has only one path from left top corner to the right bottom corner.

## Install

Build the binary from the `app` directory:

```sh
cd app && cargo build --release   # target/release/maze
```

`rust-toolchain.toml` at the repository root pins the Rust version, so `rustup`
installs and selects it on the first build and you do not have to pick a
toolchain yourself. CI reads the same file, so what you see locally is what CI
checks.

Or download a prebuilt binary from the [releases page](https://github.com/nettrash/maze-generator/releases):
`x86_64` and `arm64` builds for Linux and macOS, and an `x86_64` build for
Windows. Each archive holds the `maze` executable and the licence. The Linux
builds are static (`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`),
so they run on any distribution whatever its C library:

```sh
curl -LO https://github.com/nettrash/maze-generator/releases/download/v0.0.3/maze-0.0.3-aarch64-apple-darwin.tar.gz
tar -xzf maze-0.0.3-aarch64-apple-darwin.tar.gz && ./maze --version
```

Or pull the published image, which is built for `linux/amd64` and
`linux/arm64`:

```sh
docker run --rm nettrash/maze:0.0.3 maze --command generate --width 20 --height 10
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

# Or name the format and let stdout carry it
maze --command generate --width 30 --height 20 --format svg > maze.svg

# Reproduce a maze you liked
maze --command generate --width 20 --height 10 --algorithm wilson --seed 42

# Ask what the algorithm names actually are
maze --command list

# Pipe a maze straight into the solver
maze --command generate --output - | maze --command solve --input -

# Read the report as data rather than prose
maze --command check --input maze.txt --json
maze --command generate --width 20 --height 10 --output maze.txt --json

# Refuse a maze that has loops, when a perfect maze was what you wanted
maze --command check --input maze.txt --strict
```

Available algorithms: `backtracking`, `prim`, `kruskal`, `aldous_broder`,
`wilson`, `eller`. All of them produce a perfect maze, meaning exactly one path
connects any two cells.

`--algorithm random` lets the tool choose, and reports which one it picked.
Given a `--seed` the choice is part of the seed, so the run stays reproducible.
`--quiet` drops the banner, the progress line and the "saved to" notes.

Each option belongs to the commands `--help` lists against it, and giving it to
a command that does not read it is an error rather than being quietly ignored:
`generate --input saved.maze` would otherwise have built a brand-new maze.
Options without a `--command` are an error too, while a bare `maze` prints the
help. `--width` and `--height` default to 100 and `--algorithm` to
`backtracking`.

## Solving a maze

Because a perfect maze has exactly one route between two cells, `solve` prints
that route rather than a choice of routes. It reports the number of steps
first, then the maze with the entrance marked `S`, the exit `E`, and the
corridors in between with `.`.

With `--output`, the route is drawn into that file instead, as SVG unless the
name or `--format` asks for text, and the step count goes to stderr with the
"saved to" note. Neither drawing can be read back by `--input`, so `solve`
refuses an `--output` that is its own `--input`, which would have replaced the
maze. A maze whose exit has been sealed off fails with an error; without
`--output` it is printed unsolved first, so you can see why.

## Output formats

`generate --output` writes the plain text format by default, which is the only
format `--input` can read back, and `solve --output` writes SVG. A `.svg` name
asks for a vector drawing and a `.txt`, `.maze`, `.text` or `.ascii` name for
text, and `--format text|svg` overrides the name. Without `--output`,
`--format` writes that format to stdout, exactly as `--output -` does. SVG is
for printing and sharing: the walls are one path of strokes on a white
background, so the file stays small and sharp at any size, but it is a one-way
export. `solve` draws the route as a green line under the walls.

stdout carries the maze, the verdicts of `check` and the report of `solve`. The
banner, the progress line and the "saved to" notes go to stderr, so redirecting
stdout captures the maze without prose in front of it. What plain `generate`
prints is the rendering rather than the saved document, so use `--output` when
you want a file `--input` can read back. A reader that stops early, such as
`head`, is fine: the output just ends there.

A maze bigger than 25 million cells is refused, since the work grows with the
number of cells and a 100000x100000 request would never finish; the error says
how many cells were asked for. `aldous_broder` and `wilson` also refuse a maze
longer than 20000 cells on a side: they build it from random walks, which take
about the square of the longer side, so a 10x2500000 maze would take them
hours. A single row or column has only one perfect maze, the straight
corridor, which every algorithm carves at once whatever its length, and
`--algorithm random` only picks an algorithm that can build the shape.
`--input` reads at most 300 MB, more than the largest maze the tool makes.

### Pipes

`--input -` reads the maze document from stdin and `--output -` writes it to
stdout, so the commands compose:

```sh
maze --command generate --width 30 --height 20 --output - \
  | maze --command check --input -
```

Note the asymmetry above: plain `generate` prints the *rendering*, which is for
reading, while `--output -` writes the loadable *document* with its header,
which is what `--input -` expects. Piping the rendering by mistake fails with a
message that says so.

### Machine-readable output

`--json` replaces the prose report with a single JSON object on stdout, for
`check`, `solve`, `list` and `generate`. It implies `--quiet`.

| Command    | Fields |
| ---------- | ------ |
| `list`     | `command`, `algorithms` |
| `check`    | `command`, `source`, `width`, `height`, `perfect`, `solvable`, `unreachable_cells`, `dead_ends`, `border_openings`, `strict` |
| `solve`    | `command`, `source`, `width`, `height`, `solvable`, `steps` (`null` when the exit cannot be reached), `format` and `output` (`null` when nothing was written) |
| `generate` | `command`, `algorithm`, `seed`, `width`, `height`, `braid`, `entrance_exit`, `format`, `output` |

`generate --json` reports the seed, which is how a script keeps a maze it likes:
`--output` is required, because stdout is carrying the report, and `--output -`
is refused for the same reason. Feeding the reported seed back reproduces the
maze byte for byte. The seeds the tool picks are below 2^53, so they survive a
JSON reader that stores numbers as doubles, as JavaScript does; a `--seed` you
choose above that has to be read back as a big integer.

## Exit codes

`0` when the command did its job, `1` for anything that went wrong: bad usage,
such as an unparseable value, an option the command does not read or a missing
`--command`; an unknown command or algorithm; a file that cannot be read or
parsed; and an unreachable exit for both `check` and `solve`. The reason goes
to stderr prefixed with `error:`, so a script can branch on the status and still
show the message.

`check` fails on a maze whose exit cannot be reached. `Perfect: false` on its own
does not fail, because a braided maze is valid and reports false by design.
Pass `--strict` when a perfect maze was the requirement, and `check` fails on
loops, and on cells walled off from the entrance (reported as `Unreachable
cells`), as well; the JSON report records which gate ran as `"strict":true`.

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
and `solve` returns the shortest of the several routes available. A braided maze
is still walkable, so `check` exits 0 on it. Two shapes stay perfect anyway: a
maze one cell wide, which has no inner wall to knock down, and a very small
`--entrance-exit` maze whose only cul-de-sacs were its entrance and exit.

## Reproducible mazes

Every run picks a random seed and reports it on stderr, so passing `--seed` is
optional; `--quiet` drops that report, and `generate --json` gives it as data
instead, so a script can capture a maze and replay it. The same seed with the
same width, height, algorithm and options always produces the same maze, which
makes a maze worth keeping easy to regenerate.

That holds from one release to the next as well: `tests/reproducibility.rs`
pins the mazes a set of seeds draw, and a release that redraws any says so in
the CHANGELOG. 0.0.3 redrew Eller's mazes, braided mazes and the algorithm
`--algorithm random` picks, and nothing else.

## Maze file format

A saved maze is a plain text document: a `maze 2` marker line, a `width height`
line, and the ASCII rendering of the maze. The rendering is lossless, so loading
a saved maze reproduces the maze it came from, dead-end openings and border holes
included. Version 1 files are still readable, but one with a hole in its outer
border is rejected, since version 1 could not represent that.

A file still loads after an editor has stripped its trailing spaces, which every
maze with an exit has, or added a UTF-8 byte order mark. A malformed one is
reported with the line and column an editor shows.

## Testing

```sh
cd app
cargo test                   # unit, integration, property and command line tests
cargo test --test cli        # the command line contract on its own
cargo test --test properties # the property tests on their own
cargo test --test reproducibility # the mazes each seed draws
cargo bench                  # benchmarks, and they are not part of cargo test
```

Alongside the hand-written unit and integration tests, `tests/properties.rs`
uses [`proptest`](https://proptest-rs.github.io/proptest/) to check the
invariants that must hold for *every* maze rather than for a few fixed seeds:
every algorithm produces a perfect, solvable maze at any size including `1x1`,
`1xN` and `Nx1`; a seed always reproduces its maze; the text format round
trips exactly; braiding never disconnects a maze nor builds a wall back; and
every route `solve` reports really walks the maze without crossing a wall.

`tests/cli.rs` runs the built binary and pins the promises a library test cannot
reach: which stream each message belongs to, the exit status of every command,
the `--json` objects, the pipes, and the error messages for bad usage. It needs
nothing beyond the standard library, because Cargo hands the test the path to
the binary in `CARGO_BIN_EXE_maze`.

`tests/reproducibility.rs` hashes the mazes a fixed set of seeds draw with every
algorithm, so a change that would quietly redraw the mazes people kept by seed
fails a test instead. Redrawing them is sometimes right; then the table is
regenerated and the CHANGELOG says so.

CI runs `cargo fmt -- --check`, `cargo clippy --locked --all-targets
--all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --locked
--no-deps` and `cargo test --locked`.

## Benchmarks

`cargo bench` runs the [criterion](https://bheisler.github.io/criterion.rs/)
benchmarks in `app/benches/generation.rs` and writes an HTML report to
`app/target/criterion/report/index.html`. They are a separate target, so the
normal `cargo test` run does not pay for them.

They cover generation for all six algorithms at 50x50 and 100x100, shapes far
from square (a tall and a wide Eller maze, and a million-cell corridor for each
random walk), the cost of the `--braid` and `--entrance-exit` variants on top of
plain generation, the `Map` hot paths (`to_ascii`, `from_ascii`, `is_perfect`,
`is_solvable`, `solve`, `dead_ends`, `dead_end_count`, `neighbors`,
`neighbor_count`, `each_neighbor`, `to_svg`), file saving and loading, and the
random number generator on its own.

Expect Aldous-Broder to be the slowest by a wide margin: unlike the four
spanning-tree algorithms it revisits cells rather than carving each wall
exactly once, so its cost grows faster than the number of cells. Wilson revisits
cells too, but its walks stop as soon as they meet the maze, so it costs about
what backtracking does. On a 100x100 grid that is roughly 2.2 ms for
Aldous-Broder and 0.3 ms for Wilson, against about 0.05 to 0.3 ms for the
rest.
