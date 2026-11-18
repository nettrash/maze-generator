use chrono::Datelike;
use clap::{CommandFactory, Parser};
use std::process::ExitCode;

use maze::generator::Generator;
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

    /// File to save the generated maze to
    #[arg(long)]
    output: Option<String>,

    /// File to load a maze from, for the check and solve commands
    #[arg(long)]
    input: Option<String>,
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
            )?;
            let map = generator.generate().await?;
            match args.output.as_deref() {
                Some(path) => {
                    map.save_to_file(path)?;
                    println!("Maze saved to {path}");
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
        }
        Some("solve") => {
            intro();
            print!("Solving maze... ");
        }
        other => {
            let name = other.unwrap_or("<missing>");
            return Err(input_error(format!("unknown command: {name}")));
        }
    }
    Ok(())
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
        format!("2026-{}", year)
    };
    println!("(c) {year_str} nettrash. All rights reserved.");
    println!("This program is licensed under the MIT License.");
    println!();
}
