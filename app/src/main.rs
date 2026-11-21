use clap::{CommandFactory, Parser};
use std::fs;
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use maze::MazeError;
use maze::generator::{ALGORITHMS, Generator, MAX_CELLS, MazeOptions};
use maze::map::Map;

/// Writes a line of commentary to stderr.
///
/// `eprintln!` panics when stderr cannot be written, which is what a closed
/// pipe does to `maze --command generate 2>&1 | head -1`. Commentary is never
/// worth failing a run over, so a lost note is dropped instead.
macro_rules! note {
    ($($arg:tt)*) => {{
        let _ = writeln!(io::stderr(), $($arg)*);
    }};
}

/// The copyright year shown in the banner, kept in step with `LICENSE`.
const COPYRIGHT_YEAR: &str = "2026";

/// The `--algorithm` value that asks the tool to pick one of [`ALGORITHMS`].
///
/// It is not itself an algorithm, so it never enters the library: `ALGORITHMS`
/// is the list the benchmarks, property tests and enum check all iterate over.
const RANDOM_ALGORITHM: &str = "random";

/// The [`maze::derive_seed`] stream `--algorithm random` picks from.
const PICK_STREAM: u64 = u64::from_le_bytes(*b"pick-alg");

/// The size and algorithm `generate` uses when they are left out.
///
/// They are applied here rather than as clap defaults, so that an option the
/// user did not type stays `None` and [`reject_unused_options`] can tell it
/// from one they did.
const DEFAULT_WIDTH: u64 = 100;
const DEFAULT_HEIGHT: u64 = 100;
const DEFAULT_ALGORITHM: &str = "backtracking";

/// The `--output` value that means stdout.
const STDOUT_TARGET: &str = "-";
/// The `--input` value that means stdin.
const STDIN_SOURCE: &str = "-";

/// The most bytes `--input` reads before giving up on the input.
///
/// The largest document the generator can produce belongs to a maze one cell
/// wide and [`MAX_CELLS`] tall: `2 * MAX_CELLS + 1` lines of four characters,
/// or twelve bytes a cell once every line ends in CRLF. Twelve bytes a cell
/// plus room for the header covers every maze the tool makes. Reading used to
/// stop only at the end of the input, so `--input /dev/zero` or an endless
/// pipe grew until memory ran out: 17 GB in two seconds.
const MAX_INPUT_BYTES: u64 = 12 * MAX_CELLS + 4096;

// Command line arguments. Each option's first doc line is its summary for
// `-h`; the paragraph after the blank line is added by `--help`.
#[derive(Parser, Debug)]
#[command(
    name = "maze",
    author = "nettrash",
    version = env!("CARGO_PKG_VERSION"),
    about = "maze generator",
    long_about = None,
)]
struct Args {
    /// Command to run: generate, check, solve or list
    #[arg(long)]
    command: Option<String>,

    /// Width of the maze in cells [generate; default 100]
    #[arg(long)]
    width: Option<u64>,

    /// Height of the maze in cells [generate; default 100]
    #[arg(long)]
    height: Option<u64>,

    /// Algorithm to generate with [generate; default backtracking]
    ///
    /// One of backtracking, prim, kruskal, aldous_broder, wilson or eller,
    /// which `--command list` prints. 'random' lets the tool pick one,
    /// reproducibly from the seed.
    #[arg(long)]
    algorithm: Option<String>,

    /// Seed for the random number generator [generate]
    ///
    /// The same seed, size and algorithm always give the same maze; leave it
    /// out for a new maze every run. The seed used is reported on stderr and
    /// as "seed" by --json, so a maze worth keeping can be regenerated;
    /// --quiet drops that report.
    #[arg(long)]
    seed: Option<u64>,

    /// Where to write the maze, or '-' for stdout [generate, solve]
    ///
    /// generate writes the loadable text document, which --input reads back.
    /// solve writes the maze with its route drawn on it, which cannot be read
    /// back, so it refuses to overwrite its own --input.
    #[arg(long)]
    output: Option<String>,

    /// Format of the maze --output writes [generate, solve]
    ///
    /// Defaults to what the extension of --output says (.svg, or .txt, .maze,
    /// .text and .ascii for text), falling back to text for generate and to
    /// svg for solve. Without --output the chosen format is written to stdout,
    /// as with --output '-'.
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// File to load the maze from, or '-' for stdin [check, solve]
    #[arg(long)]
    input: Option<String>,

    /// Remove dead ends, adding loops instead [generate]
    ///
    /// The maze stays fully connected but is no longer a perfect maze. A maze
    /// one cell wide or tall has no inner wall to knock down and stays as it
    /// is.
    #[arg(long)]
    braid: bool,

    /// Open the outer border at the entrance and the exit [generate]
    #[arg(long)]
    entrance_exit: bool,

    /// Print one machine-readable JSON object instead of the prose report
    ///
    /// Works for every command and implies --quiet. generate needs --output
    /// with it, since stdout carries the report.
    #[arg(long)]
    json: bool,

    /// Suppress the banner, the progress lines and the save confirmations
    #[arg(long)]
    quiet: bool,

    /// Also fail when the maze is not perfect [check]
    ///
    /// A braided maze is a valid maze and check accepts it by default, so this
    /// is for when a perfect maze, with exactly one route between any two
    /// cells, was what was asked for. An entrance and an exit are holes in the
    /// border rather than loops, so they pass.
    #[arg(long)]
    strict: bool,
}

/// The file formats `--output` can write.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    /// Plain text: the loadable document from generate, the marked-up maze
    /// from solve
    Text,
    /// A vector drawing for printing and sharing; cannot be read back
    Svg,
}

impl OutputFormat {
    /// Guesses the format from a file extension, or `None` when the name does
    /// not say.
    fn from_path(path: &str) -> Option<Self> {
        match Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("svg") => Some(OutputFormat::Svg),
            Some("txt" | "maze" | "text" | "ascii") => Some(OutputFormat::Text),
            _ => None,
        }
    }

    /// The name shown to the user.
    fn name(self) -> &'static str {
        match self {
            OutputFormat::Text => "text",
            OutputFormat::Svg => "SVG",
        }
    }

    /// The name used in JSON, where `"SVG"` would be an odd value to match on.
    fn key(self) -> &'static str {
        match self {
            OutputFormat::Text => "text",
            OutputFormat::Svg => "svg",
        }
    }
}

/// Where each message goes.
///
/// stdout carries the maze itself, the verdicts of `check` and the report of
/// `solve`, so that `maze --command generate > maze.txt` writes a maze and
/// nothing else. Everything else — the banner, progress, the "saved to"
/// notes — is commentary and is written to stderr with `note!`, which is what
/// makes that redirect usable. `--quiet` drops the commentary entirely and
/// `--json` replaces the human report on stdout with a single JSON object,
/// implying `--quiet` so a machine never has to skip prose. Anything going
/// wrong leaves a message on stderr and exits 1; see the README for the exit
/// code contract.
pub fn main() -> ExitCode {
    // clap's own `exit` uses status 2 for bad usage, while the contract in
    // the README is 1 for anything that went wrong, so its errors are printed
    // here and mapped onto that. `--help` and `--version` arrive as errors
    // too, and they are successes.
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    let Some(command) = args.command.as_deref() else {
        // A bare `maze` asks what the tool does. Options without a command
        // are a mistake: `maze --input m.maze --strict` used to print the help
        // and exit 0, so a check whose --command had been left out passed
        // every maze.
        if std::env::args_os().len() > 1 {
            note!("error: --command is required: generate, check, solve or list (see --help)");
            return ExitCode::FAILURE;
        }
        let _ = Args::command().print_help();
        return ExitCode::SUCCESS;
    };

    match run(command, &args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            note!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: &str, args: &Args) -> Result<(), MazeError> {
    let execute: fn(&Args, bool) -> Result<(), MazeError> = match command {
        "generate" => generate,
        "check" => check,
        "solve" => solve,
        "list" => list,
        other => return Err(input_error(format!("unknown command: {other}"))),
    };
    reject_unused_options(command, args)?;

    // The banner and the progress lines are commentary, so `--quiet` drops
    // them and `--json` implies `--quiet`: whoever reads stdout as data should
    // not have to skip prose first.
    let quiet = args.quiet || args.json;
    if !quiet {
        intro();
    }
    execute(args, quiet)
}

/// Refuses an option the command does not read.
///
/// The commands share one flat set of options, and each used to ignore the
/// ones it does not read without a word: `generate --input saved.maze
/// --output saved.svg` exported a brand-new random maze, and `check --output
/// report.txt` exited 0 and wrote nothing.
fn reject_unused_options(command: &str, args: &Args) -> Result<(), MazeError> {
    let given: [(&str, bool, &[&str]); 10] = [
        ("--width", args.width.is_some(), &["generate"]),
        ("--height", args.height.is_some(), &["generate"]),
        ("--algorithm", args.algorithm.is_some(), &["generate"]),
        ("--seed", args.seed.is_some(), &["generate"]),
        ("--braid", args.braid, &["generate"]),
        ("--entrance-exit", args.entrance_exit, &["generate"]),
        ("--output", args.output.is_some(), &["generate", "solve"]),
        ("--format", args.format.is_some(), &["generate", "solve"]),
        ("--input", args.input.is_some(), &["check", "solve"]),
        ("--strict", args.strict, &["check"]),
    ];
    for (option, present, readers) in given {
        if present && !readers.contains(&command) {
            let verb = if readers.len() == 1 { "reads" } else { "read" };
            return Err(input_error(format!(
                "{option} does not apply to {command}; only {} {verb} it",
                readers.join(" and ")
            )));
        }
    }
    Ok(())
}

/// Builds a maze and either prints it or writes it to `--output`.
fn generate(args: &Args, quiet: bool) -> Result<(), MazeError> {
    let width = args.width.unwrap_or(DEFAULT_WIDTH);
    let height = args.height.unwrap_or(DEFAULT_HEIGHT);
    let requested = args.algorithm.as_deref().unwrap_or(DEFAULT_ALGORITHM);

    // Usage is checked before any work, so a mistake is reported at once
    // even when the maze would take a minute to build.
    if args.json && args.output.is_none() {
        return Err(input_error(
            "generate needs --output with --json, so the maze has somewhere to go",
        ));
    }
    let target = output_target(args);
    if let Some(target) = target {
        reject_stdout_with_json(args, target)?;
    }

    let (algorithm, seed) = resolve_algorithm(requested, args.seed, width, height);
    let generator = Generator::new(width, height, &algorithm, seed)?;
    let seed = generator.seed();
    if !quiet {
        // The library deliberately stays quiet, so the progress line lives
        // here where the tool decides what stdout is for.
        note!(
            "Generating a {width}x{height} maze using the '{algorithm}' algorithm \
             (seed {seed})..."
        );
    }
    let map = generator
        .with_options(MazeOptions {
            braid: args.braid,
            entrance_exit: args.entrance_exit,
        })
        .build();

    let Some(path) = target else {
        return emit_with(|out| {
            map.write_ascii(&mut *out)?;
            out.write_all(b"\n")
        });
    };

    let format = args
        .format
        .or_else(|| OutputFormat::from_path(path))
        .unwrap_or(OutputFormat::Text);
    // `-` is stdout. Writing the text document rather than the rendering is
    // what makes `maze --command generate --output - | maze --command solve
    // --input -` work: the rendering has no header for `--input` to read.
    match (path, format) {
        (STDOUT_TARGET, OutputFormat::Text) => emit_with(|out| map.write_text(out))?,
        (STDOUT_TARGET, OutputFormat::Svg) => emit_with(|out| map.write_svg(out, &[]))?,
        (_, OutputFormat::Text) => map.save_to_file(path)?,
        (_, OutputFormat::Svg) => map.save_svg_to_file(path)?,
    }
    if args.json {
        emit_line(&generate_json(
            &algorithm, seed, width, height, args, path, format,
        ))?;
    } else if !quiet {
        note!(
            "Maze saved to {} as {}",
            display_target(path),
            format.name()
        );
    }
    Ok(())
}

/// Reports the state of a saved maze, and fails when it cannot be walked.
fn check(args: &Args, quiet: bool) -> Result<(), MazeError> {
    let Some(source) = args.input.as_deref() else {
        return Err(input_error("the check command needs an --input file"));
    };
    let map = load_maze(source)?;
    report_loaded(&map, source, quiet);

    let perfect = map.is_perfect();
    // A perfect maze reaches every cell, the exit among them, so the second
    // walk of the maze is only needed when the first one says it is not.
    let solvable = perfect || map.is_solvable();
    let cells = (map.width() * map.height()) as usize;
    let unreachable = if perfect {
        0
    } else {
        cells - map.reachable_count()
    };
    let verdict = Verdict {
        perfect,
        solvable,
        unreachable,
        dead_ends: map.dead_end_count(),
        openings: count_border_openings(&map),
    };

    if args.json {
        emit_line(&check_json(&map, source, &verdict, args.strict))?;
    } else {
        emit(&verdict.report())?;
    }

    // Every verdict is reported before the verdict is acted on, so a script
    // sees the whole picture when the maze fails. `check` used to exit 0
    // whatever it found, which made it useless in a script and disagreed with
    // `solve`, which fails the same maze.
    //
    // `Perfect: false` on its own is not a failure: a braided maze is valid
    // and reports false by design, so only an unreachable exit makes a maze
    // unusable. `--strict` is the caller saying it wanted a perfect maze
    // anyway, which turns that finding into a failure too.
    if !solvable {
        return Err(input_error("the maze is not solvable"));
    }
    if args.strict && !perfect {
        // A maze is imperfect when it has loops or when part of it is walled
        // off, and the message used to blame loops for both.
        return Err(input_error(if unreachable > 0 {
            format!(
                "the maze is not perfect: {unreachable} of its {cells} cells cannot be \
                 reached from the entrance"
            )
        } else {
            "the maze is not perfect: it has loops, so some cells are joined by more \
             than one route"
                .to_string()
        }));
    }
    Ok(())
}

/// What `check` found.
struct Verdict {
    perfect: bool,
    solvable: bool,
    /// Cells that cannot be reached from the entrance.
    unreachable: usize,
    dead_ends: usize,
    /// Holes in the outer border.
    openings: usize,
}

impl Verdict {
    /// The prose report. The counts that are usually zero are only shown when
    /// they are not.
    fn report(&self) -> String {
        let mut report = format!(
            "Perfect: {}\nSolvable: {}\nDead ends: {}\n",
            self.perfect, self.solvable, self.dead_ends
        );
        if self.unreachable > 0 {
            report.push_str(&format!("Unreachable cells: {}\n", self.unreachable));
        }
        if self.openings > 0 {
            report.push_str(&format!("Border openings: {}\n", self.openings));
        }
        report
    }
}

/// Prints the solution route, and fails when there is none.
fn solve(args: &Args, quiet: bool) -> Result<(), MazeError> {
    let Some(source) = args.input.as_deref() else {
        return Err(input_error("the solve command needs an --input file"));
    };
    let target = output_target(args);
    if let Some(target) = target {
        reject_stdout_with_json(args, target)?;
    }
    let input = open_input(source)?;
    if let Some(target) = target {
        reject_overwriting_input(&input, target)?;
    }
    let map = read_maze(input)?;
    report_loaded(&map, source, quiet);
    let written = target.map(|target| {
        let format = args
            .format
            .or_else(|| OutputFormat::from_path(target))
            .unwrap_or(OutputFormat::Svg);
        (target, format)
    });

    let Some(route) = map.solve() else {
        if args.json {
            emit_line(&solve_json(&map, source, None, None))?;
        } else if written.is_none() {
            // Without --output, stdout is the report, so the maze is shown
            // unsolved to make the reason visible. With one, stdout is not
            // where the maze was asked to go, and through `--output -` it
            // would have been ASCII art where SVG was expected, so the error
            // alone says what went wrong.
            emit_with(|out| {
                map.write_ascii(&mut *out)?;
                out.write_all(b"\nNo solution: the exit cannot be reached from the entrance.\n")
            })?;
        }
        return Err(input_error("the maze has no solution"));
    };

    // A route of n cells is walked in n - 1 steps.
    let steps = route.len() - 1;
    match written {
        Some((target, format)) => {
            match (target, format) {
                (STDOUT_TARGET, OutputFormat::Svg) => {
                    emit_with(|out| map.write_svg(out, &route))?;
                }
                (STDOUT_TARGET, OutputFormat::Text) => emit_with(|out| {
                    map.write_ascii_with_path(&mut *out, &route)?;
                    out.write_all(b"\n")
                })?,
                (_, OutputFormat::Svg) => map.save_svg_to_file_with_path(target, &route)?,
                // The text format cannot carry a route, so the marked-up
                // rendering is written instead of the loadable document. It
                // ends in a newline like every other document the tool
                // writes.
                (_, OutputFormat::Text) => write_file(target, |out| {
                    map.write_ascii_with_path(&mut *out, &route)?;
                    out.write_all(b"\n")
                })?,
            }
            if args.json {
                emit_line(&solve_json(&map, source, Some(steps), written))?;
            } else if !quiet {
                note!(
                    "Solved in {steps} steps; saved to {} as {}",
                    display_target(target),
                    format.name()
                );
            }
        }
        None => {
            if args.json {
                emit_line(&solve_json(&map, source, Some(steps), None))?;
            } else {
                emit_with(|out| {
                    writeln!(out, "Solved in {steps} steps.")?;
                    map.write_ascii_with_path(&mut *out, &route)?;
                    out.write_all(b"\n")
                })?;
            }
        }
    }
    Ok(())
}

/// Lists the algorithm names the `--algorithm` option accepts.
fn list(args: &Args, _quiet: bool) -> Result<(), MazeError> {
    if args.json {
        let names: Vec<String> = ALGORITHMS.iter().map(|name| json_string(name)).collect();
        emit_line(&format!(
            "{{\"command\":\"list\",\"algorithms\":[{}]}}",
            names.join(",")
        ))
    } else {
        emit(&ALGORITHMS.map(|name| format!("{name}\n")).concat())
    }
}

/// Turns `--algorithm random` into one of the real names.
///
/// Handled in the tool rather than the library: `random` is a choice the CLI
/// makes, not a generator, and it must not appear in [`ALGORITHMS`]. When no
/// seed was given one is drawn here so the pick and the maze share it, which
/// keeps the reported seed enough to replay both.
///
/// Only algorithms that can build the shape are candidates, so a long thin
/// maze is never handed to a random walk that would refuse it; for any other
/// shape every algorithm is. When none can, as for a maze past
/// [`maze::generator::MAX_CELLS`], the pick is made from all of them and
/// building it reports why.
fn resolve_algorithm(
    requested: &str,
    seed: Option<u64>,
    width: u64,
    height: u64,
) -> (String, Option<u64>) {
    if requested != RANDOM_ALGORITHM {
        return (requested.to_string(), seed);
    }
    let seed = seed.unwrap_or_else(maze::random_seed);
    let able: Vec<&str> = maze::AlgorithmType::ALL
        .into_iter()
        .filter(|kind| maze::check_size(*kind, width, height).is_ok())
        .map(maze::AlgorithmType::as_str)
        .collect();
    let candidates: &[&str] = if able.is_empty() { &ALGORITHMS } else { &able };
    // The pick draws from a stream of its own. Drawing it from
    // `Rng::new(seed)`, the stream the algorithm then starts from, made the
    // pick and the algorithm's first move the same number: every
    // backtracking maze chosen this way started by carving east.
    let mut rng = maze::Rng::new(maze::derive_seed(seed, PICK_STREAM));
    let pick = candidates[rng.below(candidates.len() as u64) as usize];
    (pick.to_string(), Some(seed))
}

/// Counts the holes an [`Map::open_border`] call made in the outer border.
fn count_border_openings(map: &Map) -> usize {
    let (width, height) = (map.width() as usize, map.height() as usize);
    let mut openings = 0;
    for x in 0..width {
        openings += usize::from(map.is_border_open(x, 0, maze::Direction::North));
        openings += usize::from(map.is_border_open(x, height - 1, maze::Direction::South));
    }
    for y in 0..height {
        openings += usize::from(map.is_border_open(0, y, maze::Direction::West));
        openings += usize::from(map.is_border_open(width - 1, y, maze::Direction::East));
    }
    openings
}

/// Builds the error used for bad command line usage, so the process exits with
/// a failure status and a message on stderr rather than a panic.
fn input_error(message: impl Into<String>) -> MazeError {
    MazeError::Io(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

/// Prefixes an I/O error with the file or stream it happened on.
fn named_error(name: &str, error: io::Error) -> MazeError {
    MazeError::Io(io::Error::new(error.kind(), format!("{name}: {error}")))
}

/// An opened `--input`: how to name it, what to read, and which file it is,
/// when it is one.
struct Input {
    name: String,
    reader: Box<dyn Read>,
    file: Option<fs::Metadata>,
}

/// Opens a file, or stdin when `--input` is `-`.
fn open_input(source: &str) -> Result<Input, MazeError> {
    if source == STDIN_SOURCE {
        return Ok(Input {
            name: "stdin".to_string(),
            reader: Box::new(io::stdin().lock()),
            file: stdin_file(),
        });
    }
    let file = fs::File::open(source).map_err(|error| named_error(source, error))?;
    Ok(Input {
        name: source.to_string(),
        file: file.metadata().ok(),
        reader: Box::new(file),
    })
}

/// What stdin is on disk, when it was redirected from a file.
fn stdin_file() -> Option<fs::Metadata> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd;
        let stdin = io::stdin().as_fd().try_clone_to_owned().ok()?;
        fs::File::from(stdin).metadata().ok()
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsHandle;
        let stdin = io::stdin().as_handle().try_clone_to_owned().ok()?;
        fs::File::from(stdin).metadata().ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Reads a maze from a file, or from stdin when `--input` is `-`.
///
/// Both routes hand the text to [`Map::from_ascii`], so a piped maze is parsed
/// exactly like a saved one and `maze --command generate --output - | maze
/// --command solve --input -` works. At most [`MAX_INPUT_BYTES`] are read.
fn load_maze(source: &str) -> Result<Map, MazeError> {
    read_maze(open_input(source)?)
}

/// Reads and parses an opened `--input`; see [`load_maze`].
fn read_maze(input: Input) -> Result<Map, MazeError> {
    let name = input.name.as_str();
    let mut bytes = Vec::new();
    input
        .reader
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| named_error(name, error))?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(input_error(format!(
            "{name}: more than {MAX_INPUT_BYTES} bytes, larger than any maze this tool makes"
        )));
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        named_error(
            name,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            ),
        )
    })?;
    Map::from_ascii(&text).map_err(|error| {
        // The usual way to end up here is piping plain `generate`, whose
        // rendering has no header, so say what to pipe instead.
        let hint = if text.trim_start().starts_with('+') {
            "; that is a rendering of a maze, while `generate --output -` writes the \
             loadable document"
        } else {
            ""
        };
        named_error(name, io::Error::new(error.kind(), format!("{error}{hint}")))
    })
}

/// Refuses a `solve` whose `--output` is the `--input` it is reading.
///
/// solve writes the maze with its route drawn on it, which `--input` cannot
/// read back, so solving a maze in place used to replace it with a drawing
/// and report success: the maze was gone unless its seed was known.
///
/// The file that was opened is compared, not the names: the same maze can
/// arrive as `--input -` with stdin redirected from it, through a link, or as
/// `/dev/stdin`, whose own metadata on macOS describes the device rather than
/// the file. Only a regular file is truncated by writing it, so a target that
/// is a terminal, a pipe or a device is never refused.
fn reject_overwriting_input(input: &Input, target: &str) -> Result<(), MazeError> {
    if target == STDOUT_TARGET {
        return Ok(());
    }
    let (Some(input), Ok(output)) = (&input.file, fs::metadata(target)) else {
        return Ok(());
    };
    if output.is_file() && same_file(input, &output) {
        return Err(input_error(format!(
            "--output {target} is the --input file, and solve would replace the maze \
             with a drawing it cannot read back"
        )));
    }
    Ok(())
}

/// Whether two sets of metadata describe one file.
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(windows)]
    {
        // Stable Rust has no file index on Windows, so one file is
        // recognised by what the file system keeps once per file. Hard links
        // share that record, so all of it matches, while two separate files
        // would have had to be created within the same 100 ns. A wrong match
        // only refuses a write; it never allows one.
        use std::os::windows::fs::MetadataExt;
        a.creation_time() == b.creation_time()
            && a.last_write_time() == b.last_write_time()
            && a.file_size() == b.file_size()
            && a.file_attributes() == b.file_attributes()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (a, b);
        false
    }
}

/// Creates `path` and fills it through a buffer, naming the path in any
/// error.
fn write_file(
    path: &str,
    write: impl FnOnce(&mut BufWriter<fs::File>) -> io::Result<()>,
) -> Result<(), MazeError> {
    let file = fs::File::create(path).map_err(|error| named_error(path, error))?;
    let mut out = BufWriter::new(file);
    write(&mut out)
        .and_then(|()| out.flush())
        .map_err(|error| named_error(path, error))
}

/// How a source is named to the user: a pipe reads better as `stdin` than `-`.
fn display_source(source: &str) -> &str {
    if source == STDIN_SOURCE {
        "stdin"
    } else {
        source
    }
}

/// How a destination is named to the user: `-` means stdout.
fn display_target(target: &str) -> &str {
    if target == STDOUT_TARGET {
        "stdout"
    } else {
        target
    }
}

/// Where `generate` and `solve` write their document: `--output`, or stdout
/// when only `--format` was given.
///
/// Asking for a format and not saying where used to be ignored, so
/// `maze --command generate --format svg > maze.svg` wrote the ASCII rendering
/// into a file named for a drawing. Naming a format is a request for that
/// format, and stdout is the only place it can go.
fn output_target(args: &Args) -> Option<&str> {
    args.output
        .as_deref()
        .or(args.format.map(|_| STDOUT_TARGET))
}

/// Stops the document and `--json` both claiming stdout.
fn reject_stdout_with_json(args: &Args, target: &str) -> Result<(), MazeError> {
    if !args.json || target != STDOUT_TARGET {
        return Ok(());
    }
    Err(input_error(if args.output.is_some() {
        "--output - cannot be combined with --json, since stdout carries the report"
    } else {
        "--format without --output writes to stdout, which --json needs for the report; \
         give --output a file"
    }))
}

/// Says which maze was loaded. Commentary, so it stays off stdout and away
/// entirely when `--quiet` or `--json` asked for neither.
fn report_loaded(map: &Map, source: &str, quiet: bool) {
    if !quiet {
        note!(
            "Loaded a {}x{} maze from {}",
            map.width(),
            map.height(),
            display_source(source)
        );
    }
}

/// Renders `value` as a JSON string literal, escaping what JSON requires.
///
/// There is no serialiser in the dependency tree and these payloads are a
/// handful of numbers plus a few names, so hand-writing them beats adding a
/// crate. Names and paths come off the command line, so they are not assumed
/// to be free of quotes and backslashes.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Builds the object `generate --json` prints.
///
/// The seed is the reason it exists: a script can capture a run and replay it
/// with `--seed`, which used to mean scraping the progress line off stderr.
fn generate_json(
    algorithm: &str,
    seed: u64,
    width: u64,
    height: u64,
    args: &Args,
    path: &str,
    format: OutputFormat,
) -> String {
    format!(
        "{{\"command\":\"generate\",\"algorithm\":{},\"seed\":{seed},\
         \"width\":{width},\"height\":{height},\"braid\":{},\"entrance_exit\":{},\
         \"format\":{},\"output\":{}}}",
        json_string(algorithm),
        args.braid,
        args.entrance_exit,
        json_string(format.key()),
        json_string(path),
    )
}

/// Builds the object `check --json` prints.
fn check_json(map: &Map, source: &str, verdict: &Verdict, strict: bool) -> String {
    format!(
        "{{\"command\":\"check\",\"source\":{},\"width\":{},\"height\":{},\
         \"perfect\":{},\"solvable\":{},\"unreachable_cells\":{},\"dead_ends\":{},\
         \"border_openings\":{},\"strict\":{strict}}}",
        json_string(display_source(source)),
        map.width(),
        map.height(),
        verdict.perfect,
        verdict.solvable,
        verdict.unreachable,
        verdict.dead_ends,
        verdict.openings,
    )
}

/// Builds the object `solve --json` prints, for a solved maze (`steps` is
/// `Some`) or an unreachable exit (`None`). One builder for both so the two
/// shapes cannot drift apart. `written` is where the solved maze went and in
/// which format, reported as `null` when it was not written anywhere.
fn solve_json(
    map: &Map,
    source: &str,
    steps: Option<usize>,
    written: Option<(&str, OutputFormat)>,
) -> String {
    let (solvable, steps) = match steps {
        Some(steps) => ("true", steps.to_string()),
        None => ("false", "null".to_string()),
    };
    let (format, output) = match written {
        Some((target, format)) => (json_string(format.key()), json_string(target)),
        None => ("null".to_string(), "null".to_string()),
    };
    format!(
        "{{\"command\":\"solve\",\"source\":{},\"width\":{},\"height\":{},\
         \"solvable\":{solvable},\"steps\":{steps},\"format\":{format},\"output\":{output}}}",
        json_string(display_source(source)),
        map.width(),
        map.height(),
    )
}

/// Prints the banner. Commentary, so it goes to stderr: a banner on stdout
/// used to land in front of every maze, which made `maze --command generate >
/// maze.txt` write a file that would not load back.
fn intro() {
    let version = env!("CARGO_PKG_VERSION");
    note!("maze v{version}");
    // A copyright notice is a legal statement, not a clock reading: it says
    // when the work was published, so it stays in step with LICENSE and does
    // not wander when the machine's clock is wrong.
    note!("(c) {COPYRIGHT_YEAR} nettrash. All rights reserved.");
    note!("This program is licensed under the MIT License.");
    note!();
}

/// Writes to stdout through a buffer.
///
/// A reader that stops early, as `maze --command generate | head` does, has
/// not made the command fail, so a broken pipe ends the output quietly and
/// the command still exits on its own verdict. `println!` panicked there
/// instead, exiting 101 with a panic message about stdout. Any other write
/// error, such as a full disk behind a redirect, fails the run with a
/// message.
fn emit_with(
    write: impl FnOnce(&mut BufWriter<io::StdoutLock<'static>>) -> io::Result<()>,
) -> Result<(), MazeError> {
    let mut out = BufWriter::new(io::stdout().lock());
    match write(&mut out).and_then(|()| out.flush()) {
        Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
            Err(named_error("stdout", error))
        }
        _ => Ok(()),
    }
}

/// Writes `text` to stdout; see [`emit_with`].
fn emit(text: &str) -> Result<(), MazeError> {
    emit_with(|out| out.write_all(text.as_bytes()))
}

/// Writes `text` and a newline to stdout; see [`emit_with`].
fn emit_line(text: &str) -> Result<(), MazeError> {
    emit_with(|out| {
        out.write_all(text.as_bytes())?;
        out.write_all(b"\n")
    })
}

#[cfg(test)]
mod tests {
    use super::json_string;

    #[test]
    fn json_strings_escape_what_json_requires() {
        assert_eq!(json_string("plain"), "\"plain\"");
        assert_eq!(json_string("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(
            json_string("C:\\mazes\\a.maze"),
            "\"C:\\\\mazes\\\\a.maze\""
        );
        assert_eq!(json_string("a\nb\rc\td"), "\"a\\nb\\rc\\td\"");
        assert_eq!(json_string("\u{1}\u{1f}"), "\"\\u0001\\u001f\"");
        // Everything else, outside the ASCII range too, is valid as it is.
        assert_eq!(json_string("лабиринт 🧭"), "\"лабиринт 🧭\"");
    }
}
