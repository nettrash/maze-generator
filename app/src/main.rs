use chrono::Datelike;
use clap::{CommandFactory, Parser};
use std::io::Error;

use crate::maze::generator;

pub mod maze;

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
    #[arg(long, default_value = "basic")]
    algorithm: Option<String>,
}

#[tokio::main]
pub async fn main() -> Result<(), Error> {
    let args = Args::parse();
    if args.command.is_none() {
        let mut cmd = Args::command();
        let _ = cmd.print_help();
        return Ok(());
    }

    match args.command.as_deref() {
        Some("generate") => {
            intro();
            let generator = generator::Generator::new(
                args.width.unwrap_or(100),
                args.height.unwrap_or(100),
                args.algorithm.as_deref().unwrap_or("basic"),
            );
            generator.generate().await?;
        }
        Some("check") => {
            intro();
            print!("Checking maze... ");
        }
        Some("solve") => {
            intro();
            print!("Solving maze... ");
        }
        _ => {
            println!("Unknown command: {:?}", args.command);
        }
    }
    Ok(())
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
