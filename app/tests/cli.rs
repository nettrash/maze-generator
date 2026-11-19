//! Tests for the command line binary itself.
//!
//! These drive the built `maze` executable rather than the library, because
//! the promises the tool makes are about processes: which stream a message
//! goes to, what the exit status says, and what `--json` emits. Nothing beyond
//! the standard library is needed, since Cargo hands integration tests the
//! path to the binary through `CARGO_BIN_EXE_maze`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use maze::{ALGORITHMS, Generator, Map, MazeOptions};

/// A 1x2 maze with the wall between its two cells left standing, so the exit
/// cannot be reached. Both `check` and `solve` have to fail on it.
const SEALED: &str = "maze 2\n1 2\n+--+\n|  |\n+--+\n|  |\n+--+\n";

/// Runs the binary with `args` and captures both streams.
fn maze(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_maze"))
        .args(args)
        .output()
        .expect("the maze binary should be runnable")
}

/// Runs the binary with `args`, feeding `input` to its stdin.
fn maze_with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_maze"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the maze binary should be runnable");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(input.as_bytes())
        .expect("the binary should read its stdin");
    child.wait_with_output().expect("the binary should exit")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> Option<i32> {
    output.status.code()
}

/// A directory of its own per test, so the tests stay safe in parallel.
fn scratch(case: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("maze-cli-{case}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// Writes a maze through the library, so the fixtures come from the same code
/// the tool runs.
fn write_maze(
    case: &str,
    name: &str,
    algorithm: &str,
    width: u64,
    height: u64,
    seed: u64,
) -> PathBuf {
    let path = scratch(case).join(format!("{name}.maze"));
    Generator::new(width, height, algorithm, Some(seed))
        .expect("known algorithm")
        .build()
        .save_to_file(&path)
        .expect("a maze should be savable");
    path
}

fn write_sealed(case: &str) -> PathBuf {
    let path = scratch(case).join("sealed.maze");
    fs::write(&path, SEALED).expect("the fixture should be writable");
    path
}

/// The `--output <path>` pair for the given file.
fn to(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// --- which stream each message belongs to ----------------------------------

#[test]
fn generate_writes_only_the_maze_to_stdout() {
    let output = maze(&["--command", "generate", "--width", "6", "--height", "4"]);
    assert_eq!(code(&output), Some(0));

    let maze_text = stdout(&output);
    let first_line = maze_text.lines().next().expect("a maze was printed");
    assert!(
        first_line.starts_with("+-") && first_line.ends_with("-+"),
        "stdout should begin with the maze, not with prose: {first_line:?}"
    );
    assert!(
        !maze_text.contains("maze v"),
        "the banner leaked onto stdout: {maze_text}"
    );

    // Commentary belongs to stderr, which is what makes the redirect usable.
    let notes = stderr(&output);
    assert!(notes.contains("maze v"), "the banner went missing: {notes}");
    assert!(notes.contains("Generating a 6x4 maze"), "{notes}");
}

#[test]
fn quiet_drops_the_banner_and_the_progress_lines() {
    let output = maze(&[
        "--command",
        "generate",
        "--width",
        "4",
        "--height",
        "4",
        "--quiet",
    ]);
    assert_eq!(code(&output), Some(0));
    assert_eq!(stderr(&output), "", "nothing should be said under --quiet");
    assert!(!stdout(&output).is_empty(), "the maze itself must remain");
}

#[test]
fn json_is_quiet_by_itself() {
    let output = maze(&["--command", "list", "--json"]);
    assert_eq!(code(&output), Some(0));
    assert_eq!(stderr(&output), "", "--json implies --quiet");
}

// --- check -----------------------------------------------------------------

#[test]
fn check_exits_zero_for_a_solvable_maze() {
    let path = write_maze("check-ok", "maze", "backtracking", 9, 7, 4);
    let output = maze(&["--command", "check", "--input", &to(&path), "--quiet"]);
    assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));

    let report = stdout(&output);
    assert!(report.contains("Perfect: true"), "{report}");
    assert!(report.contains("Solvable: true"), "{report}");
    assert!(report.contains("Dead ends:"), "{report}");
}

#[test]
fn check_exits_nonzero_for_an_unsolvable_maze_and_says_why() {
    // The regression this guards: `check` used to print `Solvable: false` and
    // still exit 0, so a script could not tell a good maze from a broken one,
    // while `solve` failed on the very same file.
    let path = write_sealed("check-fail");
    let output = maze(&["--command", "check", "--input", &to(&path), "--quiet"]);
    assert_eq!(code(&output), Some(1));

    // Every verdict still has to be reported before the failure.
    let report = stdout(&output);
    assert!(report.contains("Solvable: false"), "{report}");
    assert!(
        stderr(&output).contains("not solvable"),
        "the failure should be explained: {}",
        stderr(&output)
    );
}

#[test]
fn check_json_reports_one_object_with_every_verdict() {
    let path = write_maze("check-json", "maze", "prim", 8, 8, 12);
    let output = maze(&["--command", "check", "--input", &to(&path), "--json"]);
    assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));

    let report = stdout(&output);
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines.len(), 1, "one JSON object was expected: {report}");
    for field in [
        "\"command\":\"check\"",
        "\"perfect\":true",
        "\"solvable\":true",
        "\"dead_ends\":",
        "\"border_openings\":",
        "\"strict\":false",
        "\"width\":8",
        "\"height\":8",
        &format!("\"source\":\"{}\"", to(&path)),
    ] {
        assert!(lines[0].contains(field), "{field} missing from {report}");
    }
}

#[test]
fn strict_is_an_opt_in_gate_on_perfection() {
    // A braided maze is a valid maze, so the default `check` accepts it and
    // only reports the loops. --strict is the caller saying a perfect maze was
    // what it wanted, which makes those loops a failure.
    let path = scratch("strict").join("braided.maze");
    Generator::new(14, 10, "prim", Some(21))
        .expect("known algorithm")
        .with_options(MazeOptions {
            braid: true,
            entrance_exit: false,
        })
        .build()
        .save_to_file(&path)
        .expect("a maze should be savable");

    let lenient = maze(&["--command", "check", "--input", &to(&path), "--quiet"]);
    assert_eq!(code(&lenient), Some(0), "a braided maze is valid");
    assert!(stdout(&lenient).contains("Perfect: false"));

    let strict = maze(&[
        "--command",
        "check",
        "--input",
        &to(&path),
        "--strict",
        "--quiet",
    ]);
    assert_eq!(code(&strict), Some(1));
    assert!(
        stderr(&strict).contains("not perfect"),
        "{}",
        stderr(&strict)
    );
    // The verdicts are still all reported before the failure.
    assert!(stdout(&strict).contains("Perfect: false"));

    let report = maze(&[
        "--command",
        "check",
        "--input",
        &to(&path),
        "--strict",
        "--json",
    ]);
    assert!(
        stdout(&report).contains("\"strict\":true"),
        "the report should say which gate ran: {}",
        stdout(&report)
    );
}

#[test]
fn strict_accepts_a_perfect_maze_and_solvability_still_wins() {
    let perfect = write_maze("strict-ok", "maze", "backtracking", 9, 7, 4);
    let output = maze(&[
        "--command",
        "check",
        "--input",
        &to(&perfect),
        "--strict",
        "--quiet",
    ]);
    assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));

    // An unreachable exit is the harder failure, so it is what gets reported
    // even with --strict on.
    let sealed = write_sealed("strict-sealed");
    let output = maze(&[
        "--command",
        "check",
        "--input",
        &to(&sealed),
        "--strict",
        "--quiet",
    ]);
    assert_eq!(code(&output), Some(1));
    assert!(
        stderr(&output).contains("not solvable"),
        "{}",
        stderr(&output)
    );
}

// --- solve -----------------------------------------------------------------

#[test]
fn solve_prints_the_step_count_then_the_marked_up_maze() {
    let path = write_maze("solve-ok", "maze", "kruskal", 10, 6, 77);
    let output = maze(&["--command", "solve", "--input", &to(&path), "--quiet"]);
    assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));

    let report = stdout(&output);
    let first_line = report.lines().next().expect("a report was printed");
    assert!(
        first_line.starts_with("Solved in ") && first_line.ends_with(" steps."),
        "the step count should come first: {first_line:?}"
    );
    assert!(report.contains('S') && report.contains('E'), "{report}");
}

#[test]
fn solve_exits_nonzero_when_the_exit_is_sealed_off() {
    let path = write_sealed("solve-fail");
    let output = maze(&["--command", "solve", "--input", &to(&path), "--quiet"]);
    assert_eq!(code(&output), Some(1));
    assert!(
        stderr(&output).contains("no solution"),
        "the failure should be explained: {}",
        stderr(&output)
    );
}

#[test]
fn solve_json_reports_null_steps_when_unsolvable() {
    let path = write_sealed("solve-json");
    let output = maze(&["--command", "solve", "--input", &to(&path), "--json"]);
    assert_eq!(code(&output), Some(1));

    let report = stdout(&output);
    assert_eq!(report.lines().count(), 1, "one JSON object: {report}");
    assert!(report.contains("\"solvable\":false"), "{report}");
    assert!(report.contains("\"steps\":null"), "{report}");
}

// --- pipes -----------------------------------------------------------------

#[test]
fn a_maze_generated_to_stdout_can_be_read_back_by_input_dash() {
    // The documented pipeline. `--output -` has to write the loadable document
    // rather than the plain rendering, or `--input -` has no header to parse.
    let generated = maze(&[
        "--command",
        "generate",
        "--width",
        "14",
        "--height",
        "9",
        "--algorithm",
        "wilson",
        "--seed",
        "1234",
        "--output",
        "-",
        "--quiet",
    ]);
    assert_eq!(code(&generated), Some(0));
    let document = stdout(&generated);
    assert!(
        document.starts_with("maze 2\n"),
        "the loadable document, with its header, was expected: {document}"
    );

    let solved = maze_with_stdin(
        &["--command", "solve", "--input", "-", "--quiet"],
        &document,
    );
    assert_eq!(code(&solved), Some(0), "stderr: {}", stderr(&solved));
    assert!(
        stdout(&solved).contains("Solved in "),
        "{}",
        stdout(&solved)
    );
}

#[test]
fn check_reads_a_maze_from_stdin() {
    let generated = maze(&[
        "--command",
        "generate",
        "--width",
        "5",
        "--height",
        "5",
        "--output",
        "-",
        "--quiet",
    ]);
    let checked = maze_with_stdin(
        &["--command", "check", "--input", "-", "--json"],
        &stdout(&generated),
    );
    assert_eq!(code(&checked), Some(0), "stderr: {}", stderr(&checked));
    assert!(
        stdout(&checked).contains("\"source\":\"stdin\""),
        "a pipe should be named stdin, not -: {}",
        stdout(&checked)
    );
}

#[test]
fn garbage_on_stdin_fails_without_a_panic() {
    let output = maze_with_stdin(&["--command", "check", "--input", "-"], "not a maze at all");
    assert_eq!(code(&output), Some(1));
    let notes = stderr(&output);
    assert!(
        notes.contains("stdin"),
        "the source should be named: {notes}"
    );
    assert!(!notes.contains("panicked"), "{notes}");
}

// --- generate and its output formats ---------------------------------------

#[test]
fn a_saved_maze_can_be_read_back_by_the_library() {
    let path = scratch("round-trip").join("maze.maze");
    let written = maze(&[
        "--command",
        "generate",
        "--width",
        "11",
        "--height",
        "7",
        "--algorithm",
        "eller",
        "--seed",
        "5",
        "--output",
        &to(&path),
        "--quiet",
    ]);
    assert_eq!(code(&written), Some(0), "stderr: {}", stderr(&written));

    let document = fs::read_to_string(&path).expect("the file should exist");
    let loaded = Map::from_ascii(&document).expect("the tool should write what it can read");
    assert_eq!(loaded.width(), 11);
    assert_eq!(loaded.height(), 7);
    assert!(loaded.is_perfect());
}

#[test]
fn generate_json_needs_a_file_for_the_maze() {
    let output = maze(&[
        "--command",
        "generate",
        "--width",
        "4",
        "--height",
        "4",
        "--json",
    ]);
    assert_eq!(code(&output), Some(1));
    assert!(
        stderr(&output).contains("--output"),
        "the error should name the missing option: {}",
        stderr(&output)
    );
    assert!(
        stdout(&output).is_empty(),
        "a failed run should print nothing on stdout"
    );
}

#[test]
fn json_and_output_dash_cannot_both_have_stdout() {
    let output = maze(&[
        "--command",
        "generate",
        "--width",
        "4",
        "--height",
        "4",
        "--output",
        "-",
        "--json",
    ]);
    assert_eq!(code(&output), Some(1));
    assert!(stderr(&output).contains("--json"), "{}", stderr(&output));
}

#[test]
fn generate_json_reports_the_seed_needed_to_replay_the_maze() {
    let dir = scratch("generate-json");
    let run = |name: &str| {
        let path = dir.join(format!("{name}.maze"));
        let output = maze(&[
            "--command",
            "generate",
            "--width",
            "10",
            "--height",
            "10",
            "--algorithm",
            "prim",
            "--seed",
            "4242",
            "--output",
            &to(&path),
            "--json",
        ]);
        assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));
        (path, stdout(&output))
    };

    let (first, report) = run("first");
    assert_eq!(report.lines().count(), 1, "one JSON object: {report}");
    for field in [
        "\"command\":\"generate\"",
        "\"algorithm\":\"prim\"",
        "\"seed\":4242",
        "\"width\":10",
        "\"height\":10",
        "\"braid\":false",
        "\"entrance_exit\":false",
        "\"format\":\"text\"",
    ] {
        assert!(report.contains(field), "{field} missing from {report}");
    }

    // The seed is the reason the report exists: replaying it must give the
    // same maze back byte for byte, which is what a script that captured the
    // seed relies on.
    let (second, replay) = run("second");
    // Only the output path differs between the two runs, so the replay is
    // checked by what it reports about the maze and by the bytes on disk.
    for field in [
        "\"algorithm\":\"prim\"",
        "\"seed\":4242",
        "\"width\":10",
        "\"height\":10",
    ] {
        assert!(replay.contains(field), "{field} missing from {replay}");
    }
    assert_eq!(
        fs::read_to_string(&first).expect("the first maze"),
        fs::read_to_string(&second).expect("the replay"),
        "the reported seed did not replay the maze"
    );
}

#[test]
fn the_svg_extension_writes_a_vector_drawing() {
    let path = scratch("svg").join("maze.svg");
    let output = maze(&[
        "--command",
        "generate",
        "--width",
        "6",
        "--height",
        "4",
        "--output",
        &to(&path),
        "--quiet",
    ]);
    assert_eq!(code(&output), Some(0));

    let drawing = fs::read_to_string(&path).expect("the svg should exist");
    assert!(drawing.starts_with("<svg "), "{drawing}");
    assert!(drawing.trim_end().ends_with("</svg>"), "{drawing}");
}

#[test]
fn format_overrides_what_the_extension_says() {
    let path = scratch("format").join("maze.svg");
    let output = maze(&[
        "--command",
        "generate",
        "--width",
        "5",
        "--height",
        "5",
        "--output",
        &to(&path),
        "--format",
        "text",
        "--quiet",
    ]);
    assert_eq!(code(&output), Some(0));

    // Named .svg but asked for text, so it must be the loadable format.
    let document = fs::read_to_string(&path).expect("the file should exist");
    assert!(document.starts_with("maze 2\n"), "{document}");
    assert!(!document.contains("<svg"), "{document}");
}

// --- list and the algorithm choices ----------------------------------------

#[test]
fn list_prints_the_algorithms_the_library_can_build() {
    let output = maze(&["--command", "list", "--quiet"]);
    assert_eq!(code(&output), Some(0));

    let maze_text = stdout(&output);
    let names: Vec<&str> = maze_text.lines().collect();
    assert_eq!(
        names,
        ALGORITHMS.to_vec(),
        "the list the tool advertises must be the list it accepts"
    );
}

#[test]
fn list_json_holds_the_same_names() {
    let output = maze(&["--command", "list", "--json"]);
    assert_eq!(code(&output), Some(0));

    let report = stdout(&output);
    assert!(
        report.starts_with("{\"command\":\"list\",\"algorithms\":["),
        "{report}"
    );
    for name in ALGORITHMS {
        assert!(
            report.contains(&format!("\"{name}\"")),
            "{name} missing from {report}"
        );
    }
}

#[test]
fn random_picks_a_real_algorithm_and_stays_reproducible() {
    let dir = scratch("random");
    let run = |name: &str| {
        let path = dir.join(format!("{name}.maze"));
        maze(&[
            "--command",
            "generate",
            "--width",
            "8",
            "--height",
            "8",
            "--algorithm",
            "random",
            "--seed",
            "3",
            "--output",
            &to(&path),
            "--quiet",
        ])
    };

    let first = run("first");
    let second = run("second");
    assert_eq!(code(&first), Some(0), "stderr: {}", stderr(&first));
    assert_eq!(code(&second), Some(0), "stderr: {}", stderr(&second));
    assert_eq!(
        fs::read_to_string(dir.join("first.maze")).expect("the first maze"),
        fs::read_to_string(dir.join("second.maze")).expect("the replay"),
        "'random' must be reproducible from the seed it reports"
    );

    // The pick has to be a real algorithm, and it has to be announced so the
    // run can be replayed without --algorithm random.
    let announced = maze(&[
        "--command",
        "generate",
        "--width",
        "8",
        "--height",
        "8",
        "--algorithm",
        "random",
        "--seed",
        "3",
        "--output",
        &to(&dir.join("reported.maze")),
    ]);
    let notes = stderr(&announced);
    assert!(
        ALGORITHMS
            .iter()
            .any(|name| notes.contains(&format!("'{name}' algorithm"))),
        "the chosen algorithm should be reported: {notes}"
    );
}

// --- bad usage -------------------------------------------------------------

#[test]
fn an_unknown_command_fails_with_a_named_error() {
    let output = maze(&["--command", "shuffle", "--quiet"]);
    assert_eq!(code(&output), Some(1));
    assert!(
        stderr(&output).contains("unknown command: shuffle"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_unknown_algorithm_lists_the_valid_choices() {
    let output = maze(&[
        "--command",
        "generate",
        "--width",
        "4",
        "--height",
        "4",
        "--algorithm",
        "dfs",
        "--quiet",
    ]);
    assert_eq!(code(&output), Some(1));

    let notes = stderr(&output);
    assert!(notes.contains("dfs"), "{notes}");
    for name in ALGORITHMS {
        assert!(notes.contains(name), "{name} should be suggested: {notes}");
    }
}

#[test]
fn a_missing_input_file_is_reported_rather_than_panicking() {
    for command in ["check", "solve"] {
        let output = maze(&["--command", command, "--quiet"]);
        assert_eq!(
            code(&output),
            Some(1),
            "{command} should fail without --input"
        );
        assert!(
            stderr(&output).contains("--input"),
            "{command} should name the missing option: {}",
            stderr(&output)
        );
    }
}

#[test]
fn a_missing_file_names_the_path_that_could_not_be_read() {
    let output = maze(&[
        "--command",
        "check",
        "--input",
        "/no/such/maze.maze",
        "--quiet",
    ]);
    assert_eq!(code(&output), Some(1));
    assert!(
        stderr(&output).contains("/no/such/maze.maze"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_zero_sized_maze_is_refused() {
    let output = maze(&[
        "--command",
        "generate",
        "--width",
        "0",
        "--height",
        "5",
        "--quiet",
    ]);
    assert_eq!(code(&output), Some(1));
    assert!(
        stdout(&output).is_empty(),
        "no maze may reach stdout for a rejected size"
    );
    assert!(
        stderr(&output).contains("positive width and height"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn no_command_prints_the_help_and_exits_zero() {
    let output = maze(&[]);
    assert_eq!(code(&output), Some(0));
    assert!(
        stdout(&output).contains("--command"),
        "the help text should be printed: {}",
        stdout(&output)
    );
}
