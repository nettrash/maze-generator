use chrono::Datelike;
use clap::{CommandFactory, Parser};
use std::fs;
use std::process::ExitCode;

use maze::generator::{Generator, MazeOptions};
use maze::map::Map;

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
    /// Command to execute: generate, check, solve
    #[arg(long)]
    command: Option<String>,

    /// Width of the maze
    #[arg(long, default_value = "100")]
    width: Option<u64>,

    /// Height of maze
    #[arg(long, default_value = "100")]
    height: Option<u64>,

    /// Algorithm to use for maze generation
    /// Options: backtracking, prim, kruskal, aldous_broder, wilson, eller
    #[arg(long, default_value = "backtracking")]
    algorithm: Option<String>,

    /// Seed for the random number generator
    /// Give the same seed to reproduce the same maze; omit it for a new maze
    /// every run. The seed actually used is always reported, so a maze worth
    /// keeping can be regenerated.
    #[arg(long)]
    seed: Option<u64>,

    /// File to save the generated maze to
    #[arg(long)]
    output: Option<String>,

    /// Output format for --output
    /// Defaults to the extension of --output, falling back to text
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// File to load a maze from, for the check and solve commands
    #[arg(long)]
    input: Option<String>,

    /// Remove every dead end, adding loops instead
    /// The maze stays fully connected but is no longer a perfect maze
    #[arg(long)]
    braid: bool,

    /// Open the outer border at the entrance and the exit
    #[arg(long)]
    entrance_exit: bool,
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
}

#[tokio::main]
pub async fn main() -> ExitCode {
    let args = Args::parse();
    if args.command.is_none() {
        let mut cmd = Args::command();
        let _ = cmd.print_help();
        return ExitCode::SUCCESS;
    }

    match run(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: &Args) -> Result<(), maze::MazeError> {
    match args.command.as_deref() {
        Some("generate") => {
            intro();
            let generator = Generator::new(
                args.width.unwrap_or(100),
                args.height.unwrap_or(100),
                args.algorithm.as_deref().unwrap_or("backtracking"),
                args.seed,
            )?;
            let generator = generator.with_options(MazeOptions {
                braid: args.braid,
                entrance_exit: args.entrance_exit,
            });
            let map = generator.generate().await?;
            match args.output.as_deref() {
                Some(path) => {
                    let format = args
                        .format
                        .or_else(|| OutputFormat::from_path(path))
                        .unwrap_or(OutputFormat::Text);
                    match format {
                        OutputFormat::Text => {
                            map.save_to_file(path)?;
                        }
                        OutputFormat::Svg => {
                            map.save_svg_to_file(path)?;
                        }
                    }
                    println!("Maze saved to {path} as {}", format.name());
                }
                None => println!("{map}"),
            }
        }
        Some("check") => {
            intro();
            let Some(path) = args.input.as_deref() else {
                return Err(input_error("the check command needs an --input file"));
            };
            let map = Map::load_from_file(path)?;
            println!("Loaded a {}x{} maze from {path}", map.width(), map.height());
            println!("Perfect: {}", map.is_perfect());
            println!("Solvable: {}", map.is_solvable());
            println!("Dead ends: {}", map.dead_ends().len());
            let openings = count_border_openings(&map);
            if openings > 0 {
                println!("Border openings: {openings}");
            }
        }
        Some("solve") => {
            intro();
            let Some(path) = args.input.as_deref() else {
                return Err(input_error("the solve command needs an --input file"));
            };
            let map = Map::load_from_file(path)?;
            println!("Loaded a {}x{} maze from {path}", map.width(), map.height());
            let Some(route) = map.solve() else {
                println!("{}", map.to_ascii());
                println!("No solution: the exit cannot be reached from the entrance.");
                return Err(input_error("the maze has no solution"));
            };
            // A route of n cells is walked in n - 1 steps.
            println!("Solved in {} steps.", route.len() - 1);
            match args.output.as_deref() {
                Some(target) => {
                    let format = args
                        .format
                        .or_else(|| OutputFormat::from_path(target))
                        .unwrap_or(OutputFormat::Svg);
                    match format {
                        OutputFormat::Svg => {
                            map.save_svg_to_file_with_path(target, &route)?;
                        }
                        OutputFormat::Text => {
                            // The text format cannot carry a path, so the
                            // marked-up rendering is written instead of the
                            // loadable one.
                            fs::write(target, map.to_ascii_with_path(&route))?;
                        }
                    }
                    println!("Solved maze saved to {target} as {}", format.name());
                }
                None => println!("{}", map.to_ascii_with_path(&route)),
            }
        }
        other => {
            let name = other.unwrap_or("<missing>");
            return Err(input_error(format!("unknown command: {name}")));
        }
    }
    Ok(())
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

fn intro() {
    let version = env!("CARGO_PKG_VERSION");
    println!("maze v{version}");
    let year = chrono::Utc::now().year();
    let year_str = if year == 2026 {
        "2026".to_string()
    } else {
        format!("2026-{year}")
    };
    println!("(c) {year_str} nettrash. All rights reserved.");
    println!("This program is licensed under the MIT License.");
    println!();
}
