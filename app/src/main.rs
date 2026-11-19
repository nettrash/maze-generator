use clap::{CommandFactory, Parser};
use std::fs;
use std::io::Read;
use std::process::ExitCode;

use maze::generator::{ALGORITHMS, Generator, MazeOptions};
use maze::map::Map;

/// The copyright year shown in the banner, kept in step with `LICENSE`.
const COPYRIGHT_YEAR: &str = "2026";

/// The `--algorithm` value that asks the tool to pick one of [`ALGORITHMS`].
///
/// It is not itself an algorithm, so it never enters the library: `ALGORITHMS`
/// is the list the benchmarks, property tests and enum check all iterate over.
const RANDOM_ALGORITHM: &str = "random";

// Command line arguments.
#[derive(Parser, Debug)]
#[command(
    name = "maze",
    author = "nettrash",
    version = env!("CARGO_PKG_VERSION"),
    about = "maze generator",
    long_about = None,
)]
struct Args {
    /// Command to execute: generate, check, solve, list
    #[arg(long)]
    command: Option<String>,

    /// Width of the maze
    #[arg(long, default_value = "100")]
    width: Option<u64>,

    /// Height of the maze
    #[arg(long, default_value = "100")]
    height: Option<u64>,

    /// Algorithm to use for maze generation
    /// Options: backtracking, prim, kruskal, aldous_broder, wilson, eller
    /// Pass 'random' to let the tool pick one, reproducibly from the seed
    #[arg(long, default_value = "backtracking")]
    algorithm: Option<String>,

    /// Seed for the random number generator
    /// Give the same seed to reproduce the same maze; omit it for a new maze
    /// every run. The seed actually used is always reported, so a maze worth
    /// keeping can be regenerated.
    #[arg(long)]
    seed: Option<u64>,

    /// File to save the generated maze to
    /// Pass '-' to write it to stdout as the loadable text document, which is
    /// what --input '-' reads back. solve uses this for its marked-up output
    #[arg(long)]
    output: Option<String>,

    /// Output format for --output
    /// Defaults to the extension of --output, falling back to text
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// File to load a maze from, for the check and solve commands
    /// Pass '-' to read the document from stdin instead
    #[arg(long)]
    input: Option<String>,

    /// Remove every dead end, adding loops instead
    /// The maze stays fully connected but is no longer a perfect maze
    #[arg(long)]
    braid: bool,

    /// Open the outer border at the entrance and the exit
    #[arg(long)]
    entrance_exit: bool,

    /// Print one machine-readable JSON object instead of the prose report
    /// Applies to check, solve, list and generate; implies --quiet. For
    /// generate it also requires --output, since stdout carries the report
    #[arg(long)]
    json: bool,

    /// Suppress the banner, the progress lines and the save confirmations
    #[arg(long)]
    quiet: bool,

    /// Additionally fail when the maze is not perfect
    /// Only `check` uses it. A braided or opened maze is a valid maze and
    /// `check` accepts it by default, so this is for when a perfect maze, with
    /// exactly one route between any two cells, was what was asked for
    #[arg(long)]
    strict: bool,
}

/// The file formats `--output` can write.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    /// The plain text format this tool also reads back
    Text,
    /// A vector drawing for printing and sharing; cannot be read back
    Svg,
}

impl OutputFormat {
    /// Guesses the format from a file extension, or `None` when the name does
    /// not say.
    fn from_path(path: &str) -> Option<Self> {
        match std::path::Path::new(path)
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
/// stdout carries the maze itself and the verdicts of `check`, so that
/// `maze --command generate > maze.txt` writes a maze and nothing else.
/// Everything else — the banner, progress, summaries — is commentary and is
/// written to stderr with `eprintln!`, which is what makes that redirect
/// usable. `--quiet` drops the commentary entirely and `--json` replaces the
/// human report on stdout with a single JSON object, implying `--quiet` so a
/// machine never has to skip prose. Anything going wrong leaves a message on
/// stderr and exits 1; see the README for the exit code contract.
pub fn main() -> ExitCode {
    let args = Args::parse();
    if args.command.is_none() {
        let mut cmd = Args::command();
        let _ = cmd.print_help();
        return ExitCode::SUCCESS;
    }

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), maze::MazeError> {
    // The banner and the progress lines are commentary, so `--quiet` drops
    // them and `--json` implies `--quiet`: whoever reads stdout as data should
    // not have to skip prose first.
    let quiet = args.quiet || args.json;
    if !quiet {
        intro();
    }

    match args.command.as_deref() {
        Some("generate") => generate(args, quiet),
        Some("check") => check(args, quiet),
        Some("solve") => solve(args, quiet),
        Some("list") => list(args),
        other => {
            let name = other.unwrap_or("<missing>");
            Err(input_error(format!("unknown command: {name}")))
        }
    }
}

/// Builds a maze and either prints it or writes it to `--output`.
fn generate(args: &Args, quiet: bool) -> Result<(), maze::MazeError> {
    let width = args.width.unwrap_or(100);
    let height = args.height.unwrap_or(100);
    let requested = args.algorithm.as_deref().unwrap_or("backtracking");
    let (algorithm, seed) = resolve_algorithm(requested, args.seed);

    if args.json && args.output.is_none() {
        return Err(input_error(
            "generate needs --output with --json, so the maze has somewhere to go",
        ));
    }

    let generator = Generator::new(width, height, &algorithm, seed)?;
    let seed = generator.seed();
    if !quiet {
        // The library deliberately stays quiet, so the progress line lives
        // here where the tool decides what stdout is for.
        eprintln!(
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

    let Some(path) = args.output.as_deref() else {
        println!("{map}");
        return Ok(());
    };
    reject_stdout_with_json(args, path)?;

    let format = args
        .format
        .or_else(|| OutputFormat::from_path(path))
        .unwrap_or(OutputFormat::Text);
    // `-` is stdout. Writing `to_text` rather than the rendering is what makes
    // `maze --command generate --output - | maze --command solve --input -`
    // work: the rendering has no header for `--input` to read back.
    match (path, format) {
        ("-", OutputFormat::Text) => print!("{}", map.to_text()),
        ("-", OutputFormat::Svg) => print!("{}", map.to_svg()),
        (_, OutputFormat::Text) => map.save_to_file(path)?,
        (_, OutputFormat::Svg) => map.save_svg_to_file(path)?,
    }
    if args.json {
        println!(
            "{}",
            generate_json(&algorithm, seed, width, height, args, path, format)
        );
    } else if !quiet {
        eprintln!(
            "Maze saved to {} as {}",
            display_target(path),
            format.name()
        );
    }
    Ok(())
}

/// Reports the state of a saved maze, and fails when it cannot be walked.
fn check(args: &Args, quiet: bool) -> Result<(), maze::MazeError> {
    let Some(source) = args.input.as_deref() else {
        return Err(input_error("the check command needs an --input file"));
    };
    let map = load_maze(source)?;
    report_loaded(&map, source, quiet);

    let (perfect, solvable) = (map.is_perfect(), map.is_solvable());
    let dead_ends = map.dead_end_count();
    let openings = count_border_openings(&map);

    if args.json {
        println!(
            "{}",
            check_json(
                &map,
                source,
                perfect,
                solvable,
                dead_ends,
                openings,
                args.strict
            )
        );
    } else {
        println!("Perfect: {perfect}");
        println!("Solvable: {solvable}");
        println!("Dead ends: {dead_ends}");
        if openings > 0 {
            println!("Border openings: {openings}");
        }
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
        return Err(input_error(
            "the maze is not perfect: it has loops, so it is braided or carries extra openings",
        ));
    }
    Ok(())
}

/// Prints the solution route, and fails when there is none.
fn solve(args: &Args, quiet: bool) -> Result<(), maze::MazeError> {
    let Some(source) = args.input.as_deref() else {
        return Err(input_error("the solve command needs an --input file"));
    };
    let map = load_maze(source)?;
    report_loaded(&map, source, quiet);

    let Some(route) = map.solve() else {
        if args.json {
            println!("{}", solve_json(&map, source, None));
        } else {
            println!("{}", map.to_ascii());
            println!("No solution: the exit cannot be reached from the entrance.");
        }
        return Err(input_error("the maze has no solution"));
    };

    // A route of n cells is walked in n - 1 steps.
    let steps = route.len() - 1;
    match args.output.as_deref() {
        Some(target) => {
            reject_stdout_with_json(args, target)?;
            let format = args
                .format
                .or_else(|| OutputFormat::from_path(target))
                .unwrap_or(OutputFormat::Svg);
            match (target, format) {
                ("-", OutputFormat::Svg) => print!("{}", map.to_svg_with_path(&route)),
                ("-", OutputFormat::Text) => print!("{}", map.to_ascii_with_path(&route)),
                (_, OutputFormat::Svg) => map.save_svg_to_file_with_path(target, &route)?,
                (_, OutputFormat::Text) => {
                    // The text format cannot carry a path, so the marked-up
                    // rendering is written instead of the loadable one.
                    fs::write(target, map.to_ascii_with_path(&route))?;
                }
            }
            if args.json {
                println!("{}", solve_json(&map, source, Some(steps)));
            } else if !quiet {
                eprintln!(
                    "Solved maze saved to {} as {}",
                    display_target(target),
                    format.name()
                );
            }
        }
        None => {
            if args.json {
                println!("{}", solve_json(&map, source, Some(steps)));
            } else {
                println!("Solved in {steps} steps.");
                println!("{}", map.to_ascii_with_path(&route));
            }
        }
    }
    Ok(())
}

/// Lists the algorithm names the `--algorithm` option accepts.
fn list(args: &Args) -> Result<(), maze::MazeError> {
    if args.json {
        let names: Vec<String> = ALGORITHMS.iter().map(|name| json_string(name)).collect();
        println!(
            "{{\"command\":\"list\",\"algorithms\":[{}]}}",
            names.join(",")
        );
    } else {
        for name in ALGORITHMS {
            println!("{name}");
        }
    }
    Ok(())
}

/// Turns `--algorithm random` into one of the real names.
///
/// Handled in the tool rather than the library: `random` is a choice the CLI
/// makes, not a generator, and it must not appear in [`ALGORITHMS`]. When no
/// seed was given one is drawn here so the pick and the maze share it, which
/// keeps the reported seed enough to replay both.
fn resolve_algorithm(requested: &str, seed: Option<u64>) -> (String, Option<u64>) {
    if requested != RANDOM_ALGORITHM {
        return (requested.to_string(), seed);
    }
    let seed = seed.unwrap_or_else(maze::random_seed);
    let pick = ALGORITHMS[maze::Rng::new(seed).below(ALGORITHMS.len() as u64) as usize];
    (pick.to_string(), Some(seed))
}

/// Counts the holes an [`Map::open_border`] call made in the outer border.
fn count_border_openings(map: &maze::Map) -> usize {
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
fn input_error(message: impl Into<String>) -> maze::MazeError {
    maze::MazeError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message.into(),
    ))
}

/// Reads a maze from a file, or from stdin when `--input` is `-`.
///
/// Both routes hand the text to [`Map::from_ascii`], which is what
/// [`Map::load_from_file`] does once it has read the file, so a piped maze is
/// parsed exactly like a saved one and `maze --command generate | maze
/// --command solve --input -` works.
fn load_maze(source: &str) -> Result<Map, maze::MazeError> {
    if source != "-" {
        return Ok(Map::load_from_file(source)?);
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| input_error(format!("stdin: {e}")))?;
    Map::from_ascii(&text).map_err(|e| input_error(format!("stdin: {e}")))
}

/// How a source is named to the user: a pipe reads better as `stdin` than `-`.
fn display_source(source: &str) -> &str {
    if source == "-" { "stdin" } else { source }
}

/// How a destination is named to the user: `-` means stdout.
fn display_target(target: &str) -> &str {
    if target == "-" { "stdout" } else { target }
}

/// Stops `--output -` and `--json` both claiming stdout.
fn reject_stdout_with_json(args: &Args, target: &str) -> Result<(), maze::MazeError> {
    if args.json && target == "-" {
        return Err(input_error(
            "--output - cannot be combined with --json, since stdout carries the report",
        ));
    }
    Ok(())
}

/// Says which maze was loaded. Commentary, so it stays off stdout and away
/// entirely when `--quiet` or `--json` asked for neither.
fn report_loaded(map: &Map, source: &str, quiet: bool) {
    if !quiet {
        eprintln!(
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
fn check_json(
    map: &Map,
    source: &str,
    perfect: bool,
    solvable: bool,
    dead_ends: usize,
    openings: usize,
    strict: bool,
) -> String {
    format!(
        "{{\"command\":\"check\",\"source\":{},\"width\":{},\"height\":{},\
         \"perfect\":{perfect},\"solvable\":{solvable},\"dead_ends\":{dead_ends},\
         \"border_openings\":{openings},\"strict\":{strict}}}",
        json_string(display_source(source)),
        map.width(),
        map.height(),
    )
}

/// Builds the object `solve --json` prints, for a solved maze (`steps` is
/// `Some`) or an unreachable exit (`None`). One builder for both so the two
/// shapes cannot drift apart.
fn solve_json(map: &Map, source: &str, steps: Option<usize>) -> String {
    let (solvable, steps) = match steps {
        Some(steps) => ("true", steps.to_string()),
        None => ("false", "null".to_string()),
    };
    format!(
        "{{\"command\":\"solve\",\"source\":{},\"width\":{},\"height\":{},\
         \"solvable\":{solvable},\"steps\":{steps}}}",
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
    eprintln!("maze v{version}");
    // A copyright notice is a legal statement, not a clock reading: it says
    // when the work was published, so it stays in step with LICENSE and does
    // not wander when the machine's clock is wrong.
    eprintln!("(c) {COPYRIGHT_YEAR} nettrash. All rights reserved.");
    eprintln!("This program is licensed under the MIT License.");
    eprintln!();
}
