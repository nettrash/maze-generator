//! Tests for the command line binary itself.
//!
//! These drive the built `maze` executable rather than the library, because
//! the promises the tool makes are about processes: which stream a message
//! goes to, what the exit status says, and what `--json` emits. Nothing beyond
//! the standard library is needed, since Cargo hands integration tests the
//! path to the binary through `CARGO_BIN_EXE_maze`.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use maze::{ALGORITHMS, Generator, Map, MazeOptions};

/// A 1x2 maze with the wall between its two cells left standing, so the exit
/// cannot be reached. Both `check` and `solve` have to fail on it.
const SEALED: &str = "maze 2\n1 2\n+--+\n|  |\n+--+\n|  |\n+--+\n";

/// A 2x2 maze whose only route runs along the top row and down the right
/// side, with the bottom-left cell walled in on all four sides. It is
/// solvable but not perfect, and not for having loops: one of its cells
/// cannot be reached from the entrance.
const ISLAND: &str = "maze 2\n2 2\n+--+--+\n|     |\n+--+  +\n|  |  |\n+--+--+\n";

/// A file name holding both characters JSON has to escape. Windows does not
/// allow a quote in a file name, so there the backslashes of the path itself
/// are all that is tested.
#[cfg(unix)]
const AWKWARD_NAME: &str = "say \"hi\" \\ bye";
#[cfg(not(unix))]
const AWKWARD_NAME: &str = "plain";

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

/// A directory of its own for one test, removed again when the test is over.
///
/// The directories used to be named for the case and the process id alone,
/// and nothing removed them: every run left more than a dozen behind, and a
/// run that drew a process id an earlier run had used found that run's files
/// still there. A mutant that skipped writing `maze.svg` passed that way,
/// because an old `maze.svg` was waiting for it. A guard clears anything left
/// under its name, starts from an empty directory and removes it when
/// dropped, which a failing test does too as it unwinds.
///
/// Bind the guard to a name for the whole test: a temporary one is dropped at
/// the end of its statement and takes the files with it.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(case: &str) -> Scratch {
        // The directory is removed by name, so the name has to stay a plain
        // one that cannot reach outside the temporary directory.
        assert!(
            !case.is_empty() && case.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "a scratch case is letters, digits and dashes: {case:?}"
        );
        // The tests run on parallel threads of one process, so the process id
        // alone cannot tell their directories apart; the counter does.
        static CREATED: AtomicUsize = AtomicUsize::new(0);
        let serial = CREATED.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("maze-cli-{}-{serial}-{case}", std::process::id()));
        // Only an earlier process that had this one's id can have left a
        // directory of this name, so nothing in it belongs to this run.
        match fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => panic!("could not clear {}: {error}", dir.display()),
        }
        fs::create_dir(&dir)
            .unwrap_or_else(|error| panic!("could not create {}: {error}", dir.display()));
        Scratch { dir }
    }

    /// The path `name` has inside the directory.
    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// The names in the directory, sorted, so a test can tell that a run
    /// created nothing.
    fn listing(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.dir)
            .expect("the scratch directory should be readable")
            .map(|entry| {
                entry
                    .expect("the scratch directory should be listable")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Ignored on failure: a test that is already failing must not panic
        // a second time on its way out.
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Writes a maze through the library, so the fixtures come from the same code
/// the tool runs.
fn write_maze(
    scratch: &Scratch,
    name: &str,
    algorithm: &str,
    width: u64,
    height: u64,
    seed: u64,
) -> PathBuf {
    let path = scratch.path(&format!("{name}.maze"));
    Generator::new(width, height, algorithm, Some(seed))
        .expect("known algorithm")
        .build()
        .save_to_file(&path)
        .expect("a maze should be savable");
    path
}

/// Writes `text` to the file `name` in `scratch`.
fn write_fixture(scratch: &Scratch, name: &str, text: &str) -> PathBuf {
    let path = scratch.path(name);
    fs::write(&path, text).expect("the fixture should be writable");
    path
}

fn write_sealed(scratch: &Scratch) -> PathBuf {
    write_fixture(scratch, "sealed.maze", SEALED)
}

/// A path as a command line argument.
fn to(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The maze saved at `path` and its route from the entrance to the exit, as
/// the library works them out, to hold what the tool reports against. A
/// route of n cells is walked in n - 1 steps.
fn library_solution(path: &Path) -> (Map, Vec<(usize, usize)>) {
    let map = Map::load_from_file(path).expect("the fixture should load");
    let route = map.solve().expect("the fixture should be solvable");
    (map, route)
}

/// The N of "Solved in N steps", wherever in `text` the tool said it.
fn steps_in(text: &str) -> usize {
    let (_, after) = text
        .split_once("Solved in ")
        .unwrap_or_else(|| panic!("no step count in {text:?}"));
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("no step count in {text:?}"))
}

/// `value` as a JSON string, escaped the way the tool escapes the paths it
/// reports. Only the backslash and the quote are handled: no path these tests
/// make holds anything else JSON needs escaped, and a Windows path is full of
/// backslashes.
fn json_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The whole number `key` holds in the one-line JSON object `report`.
fn json_number(report: &str, key: &str) -> u64 {
    let (_, after) = report
        .split_once(&format!("\"{key}\":"))
        .unwrap_or_else(|| panic!("{key} missing from {report}"));
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    // All of the value: `1.5e18` would read as 1, and is no seed to replay.
    assert!(
        matches!(after[digits.len()..].chars().next(), Some(',' | '}')),
        "{key} is not a whole number in {report}"
    );
    digits
        .parse()
        .unwrap_or_else(|_| panic!("{key} is not a number in {report}"))
}

/// The string `key` holds in the one-line JSON object `report`, for the plain
/// names that need no escaping.
fn json_text<'a>(report: &'a str, key: &str) -> &'a str {
    let (_, after) = report
        .split_once(&format!("\"{key}\":\""))
        .unwrap_or_else(|| panic!("{key} missing from {report}"));
    let (value, _) = after
        .split_once('"')
        .unwrap_or_else(|| panic!("{key} is not a closed string in {report}"));
    assert!(!value.contains('\\'), "{key} is not a plain name: {report}");
    value
}

// --- the scratch directories ------------------------------------------------

/// The guard is what keeps one run's files from standing in for another's, so
/// it is held to its promise: an empty directory to start from, and nothing
/// left once the test is done with it.
#[test]
fn a_scratch_directory_starts_empty_and_is_gone_once_dropped() {
    let scratch = Scratch::new("guard");
    assert!(
        scratch.listing().is_empty(),
        "a new scratch directory held {:?}",
        scratch.listing()
    );
    fs::write(scratch.path("left-behind.txt"), "x").expect("the directory should be writable");
    let dir = scratch.dir.clone();
    drop(scratch);
    assert!(!dir.exists(), "{} was left behind", dir.display());
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

/// `--quiet` promises silence on stderr, and saving is where the "saved to"
/// notes come from, so they have to go as well. The same save without
/// `--quiet` does say it, so the silence is the flag's doing.
#[test]
fn quiet_drops_the_save_confirmations_too() {
    let dir = Scratch::new("quiet-save");
    let loud = dir.path("loud.maze");
    let saved = maze(&[
        "--command",
        "generate",
        "--width",
        "4",
        "--height",
        "4",
        "--output",
        &to(&loud),
    ]);
    assert_eq!(code(&saved), Some(0), "stderr: {}", stderr(&saved));
    assert!(
        stderr(&saved).contains("saved to"),
        "a save should be confirmed without --quiet: {}",
        stderr(&saved)
    );

    let quiet = dir.path("quiet.maze");
    let drawing = dir.path("solved.svg");
    for (args, written) in [
        (
            &[
                "--command",
                "generate",
                "--width",
                "4",
                "--height",
                "4",
                "--output",
                &to(&quiet),
                "--quiet",
            ][..],
            &quiet,
        ),
        (
            &[
                "--command",
                "solve",
                "--input",
                &to(&loud),
                "--output",
                &to(&drawing),
                "--quiet",
            ],
            &drawing,
        ),
    ] {
        let output = maze(args);
        assert_eq!(code(&output), Some(0), "{args:?}: {}", stderr(&output));
        assert_eq!(
            stderr(&output),
            "",
            "{args:?} should say nothing under --quiet"
        );
        assert_eq!(stdout(&output), "", "{args:?} wrote its document to a file");
        assert!(written.exists(), "{args:?} should still write the file");
    }
}

/// `maze --command generate | head -1` used to panic: `println!` treats a
/// closed pipe as fatal, so the run ended with "failed printing to stdout:
/// Broken pipe" and status 101.
#[test]
fn a_reader_that_stops_early_does_not_make_the_tool_panic() {
    // 300x300 is about 540 kB, far more than a pipe buffers, so the tool is
    // still writing when the reader goes away.
    let mut child = Command::new(env!("CARGO_BIN_EXE_maze"))
        .args(["--command", "generate", "--width", "300", "--height", "300"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the maze binary should be runnable");
    let mut first = [0u8; 16];
    child
        .stdout
        .take()
        .expect("stdout is piped")
        .read_exact(&mut first)
        .expect("the maze should start arriving");
    // Dropping the read end above closed the pipe under the writer.
    let output = child.wait_with_output().expect("the binary should exit");

    assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));
    let notes = stderr(&output);
    assert!(!notes.contains("panicked"), "{notes}");
    assert!(
        !notes.contains("error:"),
        "a closed pipe is not an error: {notes}"
    );
    assert!(first.starts_with(b"+--"), "{first:?}");
}

/// `--json` implies `--quiet` for every command, not just `list`: whoever
/// reads stdout as data should find one object there and nothing on stderr
/// unless something went wrong. The progress line and the "Loaded" and
/// "saved to" notes are all commentary that has to stay away.
#[test]
fn json_is_quiet_by_itself() {
    let dir = Scratch::new("json-quiet");
    let saved = to(&dir.path("maze.maze"));
    let drawing = to(&dir.path("solved.svg"));
    for args in [
        &["--command", "list", "--json"][..],
        &[
            "--command",
            "generate",
            "--width",
            "4",
            "--height",
            "4",
            "--output",
            &saved,
            "--json",
        ],
        &["--command", "check", "--input", &saved, "--json"],
        &["--command", "solve", "--input", &saved, "--json"],
        &[
            "--command",
            "solve",
            "--input",
            &saved,
            "--output",
            &drawing,
            "--json",
        ],
    ] {
        let output = maze(args);
        assert_eq!(code(&output), Some(0), "{args:?}: {}", stderr(&output));
        assert_eq!(stderr(&output), "", "{args:?}: --json implies --quiet");
        assert_eq!(
            stdout(&output).lines().count(),
            1,
            "{args:?}: one JSON object was expected: {}",
            stdout(&output)
        );
    }
}

// --- check -----------------------------------------------------------------

#[test]
fn check_exits_zero_for_a_solvable_maze() {
    let dir = Scratch::new("check-ok");
    let path = write_maze(&dir, "maze", "backtracking", 9, 7, 4);
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
    let dir = Scratch::new("check-fail");
    let path = write_sealed(&dir);
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

/// Every verdict is in the one object, the dead-end count equal to the
/// library's. The source is expected escaped the way the tool escapes it:
/// compared raw, it failed wherever the temporary path needs escaping, which
/// is every Windows path.
#[test]
fn check_json_reports_one_object_with_every_verdict() {
    let dir = Scratch::new("check-json");
    let path = write_maze(&dir, "maze", "prim", 8, 8, 12);
    let output = maze(&["--command", "check", "--input", &to(&path), "--json"]);
    assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));

    let report = stdout(&output);
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines.len(), 1, "one JSON object was expected: {report}");
    for field in [
        "\"command\":\"check\"",
        "\"perfect\":true",
        "\"solvable\":true",
        "\"unreachable_cells\":0",
        "\"dead_ends\":",
        "\"border_openings\":0",
        "\"strict\":false",
        "\"width\":8",
        "\"height\":8",
        &format!("\"source\":{}", json_string(&to(&path))),
    ] {
        assert!(lines[0].contains(field), "{field} missing from {report}");
    }
    let (map, _) = library_solution(&path);
    assert_eq!(
        json_number(&report, "dead_ends"),
        map.dead_end_count() as u64,
        "{report}"
    );
}

#[test]
fn strict_is_an_opt_in_gate_on_perfection() {
    // A braided maze is a valid maze, so the default `check` accepts it and
    // only reports the loops. --strict is the caller saying a perfect maze was
    // what it wanted, which makes those loops a failure.
    let dir = Scratch::new("strict");
    let path = dir.path("braided.maze");
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
    let notes = stderr(&strict);
    assert!(notes.contains("not perfect"), "{notes}");
    // Loops are what make a braided maze imperfect, and the message has to
    // keep saying so now that a walled-off cell gets a message of its own.
    assert!(
        notes.contains("loops") && !notes.contains("cannot be reached"),
        "the loops should be blamed: {notes}"
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
    let dir = Scratch::new("strict-ok");
    let perfect = write_maze(&dir, "maze", "backtracking", 9, 7, 4);
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
    let sealed = write_sealed(&dir);
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

/// A maze is imperfect when it has loops or when part of it is walled off,
/// and `--strict` used to blame loops for both. The island has no loops and
/// one cell nothing leads to: the report counts that cell in both forms, the
/// default check passes the maze because its exit can still be reached, and
/// `--strict` fails it for the cell rather than for loops it does not have.
#[test]
fn a_walled_off_cell_is_counted_and_fails_only_under_strict() {
    let dir = Scratch::new("island");
    let island = to(&write_fixture(&dir, "island.maze", ISLAND));

    let prose = maze(&["--command", "check", "--input", &island, "--quiet"]);
    assert_eq!(
        code(&prose),
        Some(0),
        "the exit can be reached, so the maze passes: {}",
        stderr(&prose)
    );
    let report = stdout(&prose);
    for line in ["Perfect: false", "Solvable: true", "Unreachable cells: 1"] {
        assert!(
            report.lines().any(|reported| reported == line),
            "{line:?} missing from {report}"
        );
    }

    let json = maze(&["--command", "check", "--input", &island, "--json"]);
    assert_eq!(code(&json), Some(0), "stderr: {}", stderr(&json));
    let report = stdout(&json);
    assert_eq!(json_number(&report, "unreachable_cells"), 1, "{report}");
    for field in ["\"perfect\":false", "\"solvable\":true"] {
        assert!(report.contains(field), "{field} missing from {report}");
    }

    let strict = maze(&[
        "--command",
        "check",
        "--input",
        &island,
        "--strict",
        "--quiet",
    ]);
    assert_eq!(code(&strict), Some(1), "an imperfect maze fails --strict");
    let notes = stderr(&strict);
    assert!(
        notes.contains("cannot be reached"),
        "the walled-off cell should be the reason: {notes}"
    );
    assert!(
        !notes.contains("loop"),
        "the island has no loops to blame: {notes}"
    );
}

/// `--braid` had only been checked through the library. Through the tool, a
/// braided maze has to come back from `check` as valid but not perfect, with
/// no dead end left and nothing walled off, as the README says it does.
#[test]
fn a_braided_maze_from_the_tool_checks_as_imperfect_with_no_dead_ends() {
    let dir = Scratch::new("braid");
    let path = to(&dir.path("braided.maze"));
    let generated = maze(&[
        "--command",
        "generate",
        "--width",
        "7",
        "--height",
        "5",
        "--seed",
        "2",
        "--braid",
        "--output",
        &path,
        "--json",
    ]);
    assert_eq!(code(&generated), Some(0), "stderr: {}", stderr(&generated));
    assert!(
        stdout(&generated).contains("\"braid\":true"),
        "{}",
        stdout(&generated)
    );

    let checked = maze(&["--command", "check", "--input", &path, "--json"]);
    assert_eq!(
        code(&checked),
        Some(0),
        "a braided maze is valid: {}",
        stderr(&checked)
    );
    let report = stdout(&checked);
    for field in ["\"perfect\":false", "\"solvable\":true"] {
        assert!(report.contains(field), "{field} missing from {report}");
    }
    assert_eq!(json_number(&report, "dead_ends"), 0, "{report}");
    assert_eq!(
        json_number(&report, "unreachable_cells"),
        0,
        "braiding must not wall anything off: {report}"
    );
}

/// `--entrance-exit` had likewise only been checked through the library.
/// `check` has to count the two holes it makes, and since they are holes in
/// the border rather than loops, the maze is still perfect and passes
/// `--strict`.
#[test]
fn entrance_exit_from_the_tool_opens_the_border_twice_and_stays_perfect() {
    let dir = Scratch::new("entrance-exit");
    let path = to(&dir.path("open.maze"));
    let generated = maze(&[
        "--command",
        "generate",
        "--width",
        "6",
        "--height",
        "4",
        "--seed",
        "3",
        "--entrance-exit",
        "--output",
        &path,
        "--quiet",
    ]);
    assert_eq!(code(&generated), Some(0), "stderr: {}", stderr(&generated));

    let checked = maze(&["--command", "check", "--input", &path, "--json"]);
    assert_eq!(code(&checked), Some(0), "stderr: {}", stderr(&checked));
    let report = stdout(&checked);
    assert_eq!(json_number(&report, "border_openings"), 2, "{report}");
    assert!(report.contains("\"perfect\":true"), "{report}");

    let strict = maze(&[
        "--command",
        "check",
        "--input",
        &path,
        "--strict",
        "--quiet",
    ]);
    assert_eq!(
        code(&strict),
        Some(0),
        "holes in the border are not loops: {}",
        stderr(&strict)
    );
    assert!(
        stdout(&strict)
            .lines()
            .any(|line| line == "Border openings: 2"),
        "{}",
        stdout(&strict)
    );
}

// --- solve -----------------------------------------------------------------

/// The report is the step count, then the maze with its route marked on it.
/// The check for the markers used to be vacuous: it looked for an `S`
/// anywhere in the report, and "Solved" has one. The maze lines are now held
/// against the library's own rendering of the route, and the count against
/// the library's walk.
#[test]
fn solve_prints_the_step_count_then_the_marked_up_maze() {
    let dir = Scratch::new("solve-ok");
    let path = write_maze(&dir, "maze", "kruskal", 10, 6, 77);
    let output = maze(&["--command", "solve", "--input", &to(&path), "--quiet"]);
    assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));

    let report = stdout(&output);
    let (first_line, drawing) = report
        .split_once('\n')
        .expect("a count line and then the maze");
    assert!(
        first_line.starts_with("Solved in ") && first_line.ends_with(" steps."),
        "the step count should come first: {first_line:?}"
    );
    let (map, route) = library_solution(&path);
    assert_eq!(
        steps_in(first_line),
        route.len() - 1,
        "the library walks the maze in a different number of steps: {first_line}"
    );
    assert!(
        drawing.contains('S') && drawing.contains('E'),
        "the maze should mark the entrance and the exit: {drawing}"
    );
    assert_eq!(
        drawing,
        format!("{}\n", map.to_ascii_with_path(&route)),
        "the maze should be drawn with the library's route on it"
    );
}

/// Without `--output`, stdout is the report, so an unsolvable maze is shown
/// there unsolved with a line saying why, and the run still fails.
#[test]
fn solve_exits_nonzero_when_the_exit_is_sealed_off() {
    let dir = Scratch::new("solve-fail");
    let path = write_sealed(&dir);
    let output = maze(&["--command", "solve", "--input", &to(&path), "--quiet"]);
    assert_eq!(code(&output), Some(1));
    assert!(
        stderr(&output).contains("no solution"),
        "the failure should be explained: {}",
        stderr(&output)
    );

    let report = stdout(&output);
    let unsolved = Map::from_ascii(SEALED)
        .expect("the fixture should parse")
        .to_ascii();
    assert!(
        report.starts_with(&format!("{unsolved}\n")),
        "the maze should be shown unsolved: {report}"
    );
    assert!(
        report.contains("No solution"),
        "stdout should say why: {report}"
    );
}

#[test]
fn solve_json_reports_null_steps_when_unsolvable() {
    let dir = Scratch::new("solve-json");
    let path = write_sealed(&dir);
    let output = maze(&["--command", "solve", "--input", &to(&path), "--json"]);
    assert_eq!(code(&output), Some(1));

    let report = stdout(&output);
    assert_eq!(report.lines().count(), 1, "one JSON object: {report}");
    // Nothing was written anywhere, which the report says rather than
    // leaving the fields out.
    for field in [
        "\"solvable\":false",
        "\"steps\":null",
        "\"format\":null",
        "\"output\":null",
    ] {
        assert!(report.contains(field), "{field} missing from {report}");
    }
}

/// With `--output`, stdout is not where the maze was asked to go, yet an
/// unsolvable maze used to be printed there unsolved anyway: through
/// `--output -` that put ASCII art where SVG was expected. Now the error
/// alone says what went wrong, and no file is made for a solution that does
/// not exist.
#[test]
fn an_unsolvable_maze_writes_nothing_where_the_solution_was_asked_to_go() {
    let dir = Scratch::new("unsolvable-output");
    let sealed = to(&write_sealed(&dir));
    let drawing = to(&dir.path("solved.svg"));
    let text = to(&dir.path("solved.txt"));
    for rest in [
        &["--output", "-"][..],
        &["--output", "-", "--format", "text"],
        &["--format", "svg"],
        &["--output", &drawing],
        &["--output", &text],
    ] {
        let output = maze(&[&["--command", "solve", "--input", &sealed][..], rest].concat());
        assert_eq!(code(&output), Some(1), "{rest:?}: {}", stderr(&output));
        assert!(
            output.stdout.is_empty(),
            "{rest:?} wrote to stdout: {}",
            stdout(&output)
        );
        assert!(
            stderr(&output).contains("no solution"),
            "{rest:?} should say why: {}",
            stderr(&output)
        );
        assert_eq!(
            dir.listing(),
            ["sealed.maze"],
            "{rest:?} made a file for a solution that does not exist"
        );
    }
}

/// With `--output`, stdout is no longer the report, and saving used to drop
/// the step count: the note only said where the drawing went. Now the note
/// carries it and `--json` reports it as `steps`, both the library's count.
/// The drawing carries the route exactly once, drawn before the walls so that
/// they stay on top of it.
#[test]
fn solving_into_an_svg_reports_the_steps_and_draws_the_route_under_the_walls() {
    let dir = Scratch::new("solve-svg");
    let input = to(&write_maze(&dir, "maze", "wilson", 9, 6, 31));
    let svg = dir.path("solved.svg");
    let output = maze(&[
        "--command",
        "solve",
        "--input",
        &input,
        "--output",
        &to(&svg),
    ]);
    assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));
    assert!(
        output.stdout.is_empty(),
        "the drawing went to a file: {}",
        stdout(&output)
    );

    let (_, route) = library_solution(Path::new(&input));
    let notes = stderr(&output);
    assert_eq!(steps_in(&notes), route.len() - 1, "{notes}");
    assert!(
        notes.contains("saved to") && notes.contains(&to(&svg)),
        "the note should say where the drawing went: {notes}"
    );

    let drawing = fs::read_to_string(&svg).expect("the drawing should exist");
    assert_eq!(
        drawing.matches("<polyline").count(),
        1,
        "the route should be drawn once: {drawing}"
    );
    let route_at = drawing.find("<polyline").expect("the route");
    let walls_at = drawing.find("<path").expect("the walls should be drawn");
    assert!(
        route_at < walls_at,
        "the route should be drawn under the walls: {drawing}"
    );
    let points = drawing[route_at..]
        .split_once("points=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(points, _)| points)
        .expect("the route should list its points");
    assert_eq!(
        points.split(' ').count(),
        route.len(),
        "the line should pass through every cell of the route: {points}"
    );

    let json = maze(&[
        "--command",
        "solve",
        "--input",
        &input,
        "--output",
        &to(&svg),
        "--json",
    ]);
    assert_eq!(code(&json), Some(0), "stderr: {}", stderr(&json));
    let report = stdout(&json);
    assert_eq!(
        json_number(&report, "steps"),
        (route.len() - 1) as u64,
        "{report}"
    );
    assert_eq!(json_text(&report, "format"), "svg", "{report}");
    let output_field = format!("\"output\":{}", json_string(&to(&svg)));
    assert!(
        report.contains(&output_field),
        "{output_field} missing from {report}"
    );
}

/// solve writes the maze with its route drawn on it, which `--input` cannot
/// read back, so solving a maze into its own file used to replace the maze
/// with a drawing and report success: the maze was gone unless its seed was
/// known. The refusal compares files, not spellings, so the same file named
/// another way is refused too.
#[test]
fn solve_refuses_to_overwrite_its_own_input() {
    let dir = Scratch::new("overwrite");
    let input = write_maze(&dir, "maze", "kruskal", 6, 5, 9);
    let original = fs::read(&input).expect("the fixture should be readable");
    let source = to(&input);
    let respelled = to(&dir.path(".").join("maze.maze"));
    for rest in [
        &["--output", &source][..],
        &["--output", &source, "--format", "svg"],
        &["--output", &respelled],
    ] {
        let output = maze(&[&["--command", "solve", "--input", &source][..], rest].concat());
        assert_eq!(
            code(&output),
            Some(1),
            "{rest:?} should be refused: {}",
            stderr(&output)
        );
        let notes = stderr(&output);
        assert!(
            notes.contains("--input") && notes.contains("--output"),
            "{rest:?}: the error should say the output is the input: {notes}"
        );
        assert!(
            output.stdout.is_empty(),
            "{rest:?} printed: {}",
            stdout(&output)
        );
        let now = fs::read(&input).expect("the input should still be there");
        assert!(
            now == original,
            "{rest:?} changed the input:\n{}",
            String::from_utf8_lossy(&now)
        );
        assert_eq!(dir.listing(), ["maze.maze"], "{rest:?} wrote a file");
    }
}

/// The same refusal through links. Creating a file through a symlink writes to
/// the file it points at, and a hard link is the input under another name, so
/// either one as `--output` would destroy the input all the same.
#[cfg(unix)]
#[test]
fn solve_refuses_to_overwrite_its_input_through_a_link() {
    let dir = Scratch::new("overwrite-link");
    let input = write_maze(&dir, "maze", "kruskal", 6, 5, 9);
    let original = fs::read(&input).expect("the fixture should be readable");
    let symlink = dir.path("link.svg");
    std::os::unix::fs::symlink(&input, &symlink).expect("a symlink should be creatable");
    let hard_link = dir.path("twin.maze");
    fs::hard_link(&input, &hard_link).expect("a hard link should be creatable");
    let before = dir.listing();

    for (source, target) in [(&input, &symlink), (&symlink, &input), (&input, &hard_link)] {
        let case = format!("--input {} --output {}", to(source), to(target));
        let output = maze(&[
            "--command",
            "solve",
            "--input",
            &to(source),
            "--output",
            &to(target),
        ]);
        assert_eq!(
            code(&output),
            Some(1),
            "{case} should be refused: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("--input"),
            "{case}: the error should say the output is the input: {}",
            stderr(&output)
        );
        let now = fs::read(&input).expect("the input should still be there");
        assert!(
            now == original,
            "{case} changed the input:\n{}",
            String::from_utf8_lossy(&now)
        );
        assert_eq!(dir.listing(), before, "{case} wrote a file");
    }
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

/// Plain `generate` prints the rendering, which has no header, so piping it
/// into `--input -` is the usual way to end up with a parse error. The error
/// says what to pipe instead. Other garbage gets the plain error, where the
/// hint would only mislead.
#[test]
fn piping_the_rendering_into_solve_suggests_output_dash() {
    let rendering = maze(&[
        "--command",
        "generate",
        "--width",
        "5",
        "--height",
        "4",
        "--quiet",
    ]);
    assert_eq!(code(&rendering), Some(0), "stderr: {}", stderr(&rendering));

    let piped = maze_with_stdin(
        &["--command", "solve", "--input", "-", "--quiet"],
        &stdout(&rendering),
    );
    assert_eq!(
        code(&piped),
        Some(1),
        "a rendering cannot be loaded: {}",
        stdout(&piped)
    );
    assert!(piped.stdout.is_empty(), "{}", stdout(&piped));
    let notes = stderr(&piped);
    assert!(
        notes.contains("stdin") && notes.contains("--output -"),
        "the error should say what to pipe instead: {notes}"
    );

    let garbage = maze_with_stdin(
        &["--command", "solve", "--input", "-", "--quiet"],
        "not a maze at all",
    );
    assert_eq!(code(&garbage), Some(1));
    assert!(
        !stderr(&garbage).contains("--output -"),
        "the hint is for a rendering only: {}",
        stderr(&garbage)
    );
}

// --- generate and its output formats ---------------------------------------

#[test]
fn a_saved_maze_can_be_read_back_by_the_library() {
    let dir = Scratch::new("round-trip");
    let path = dir.path("maze.maze");
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
    let dir = Scratch::new("generate-json");
    let run = |name: &str| {
        let path = dir.path(&format!("{name}.maze"));
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

/// An unseeded run is the one whose seed matters: the tool drew it, and for
/// `--algorithm random` drew the pick from it too, so the reported seed and
/// algorithm are all there is to replay the maze with. Fed back, they have to
/// give the same maze byte for byte. The seed also has to survive a JSON
/// reader that keeps numbers as doubles, so it has to stay below 2^53.
#[test]
fn the_seed_reported_for_an_unseeded_run_replays_the_maze() {
    let dir = Scratch::new("unseeded");
    let generate = |algorithm: &str, seed: Option<u64>, path: &Path| {
        let (seed, path) = (seed.map(|seed| seed.to_string()), to(path));
        let mut args = vec![
            "--command",
            "generate",
            "--width",
            "12",
            "--height",
            "9",
            "--algorithm",
            algorithm,
            "--output",
            &path,
            "--json",
        ];
        if let Some(seed) = &seed {
            args.extend(["--seed", seed]);
        }
        let output = maze(&args);
        assert_eq!(code(&output), Some(0), "{args:?}: {}", stderr(&output));
        stdout(&output)
    };
    let same_maze = |a: &Path, b: &Path| {
        fs::read(a).expect("the first maze should exist")
            == fs::read(b).expect("the replay should exist")
    };

    let mut seeds = Vec::new();
    for requested in ["eller", "random"] {
        let first = dir.path(&format!("{requested}.maze"));
        let report = generate(requested, None, &first);
        let seed = json_number(&report, "seed");
        let algorithm = json_text(&report, "algorithm");
        assert!(
            seed < 1 << 53,
            "seed {seed} would not survive a JSON double: {report}"
        );
        assert!(
            ALGORITHMS.contains(&algorithm),
            "the report should name a real algorithm: {report}"
        );
        if requested != "random" {
            assert_eq!(algorithm, requested, "{report}");
        }

        let replay = dir.path(&format!("{requested}-replay.maze"));
        generate(algorithm, Some(seed), &replay);
        assert!(
            same_maze(&first, &replay),
            "--seed {seed} --algorithm {algorithm} did not replay the {requested} maze"
        );
        if requested == "random" {
            // The pick is part of the seed, so the seed alone replays it.
            let again = dir.path("random-again.maze");
            generate("random", Some(seed), &again);
            assert!(
                same_maze(&first, &again),
                "--seed {seed} --algorithm random did not pick {algorithm} again"
            );
        }
        seeds.push(seed);
    }
    assert_ne!(seeds[0], seeds[1], "two unseeded runs drew the same seed");
}

/// Paths come off the command line, so the reports cannot assume they hold
/// nothing JSON needs escaped. A quote left as it is would end the string
/// early, and a backslash would start an escape, so every report that names a
/// path has to escape it.
#[test]
fn json_reports_escape_the_paths_they_name() {
    let dir = Scratch::new("json-escape");
    let saved = dir.path(&format!("{AWKWARD_NAME}.maze"));
    let drawn = dir.path(&format!("{AWKWARD_NAME}.svg"));
    let (saved_arg, drawn_arg) = (to(&saved), to(&drawn));

    let generated = maze(&[
        "--command",
        "generate",
        "--width",
        "4",
        "--height",
        "3",
        "--seed",
        "1",
        "--output",
        &saved_arg,
        "--json",
    ]);
    assert_eq!(code(&generated), Some(0), "stderr: {}", stderr(&generated));
    assert!(
        saved.exists(),
        "the maze should be saved under the name given"
    );
    let report = stdout(&generated);
    let output_field = format!("\"output\":{}", json_string(&saved_arg));
    assert!(
        report.contains(&output_field),
        "{output_field} missing from {report}"
    );
    // Spelled out by hand as well, so the expectation does not rest on the
    // test's own escaping alone.
    #[cfg(unix)]
    assert!(
        report.contains(r#"say \"hi\" \\ bye.maze""#),
        "the quote and the backslash should be escaped: {report}"
    );

    let checked = maze(&["--command", "check", "--input", &saved_arg, "--json"]);
    assert_eq!(code(&checked), Some(0), "stderr: {}", stderr(&checked));
    let source_field = format!("\"source\":{}", json_string(&saved_arg));
    assert!(
        stdout(&checked).contains(&source_field),
        "{source_field} missing from {}",
        stdout(&checked)
    );

    let solved = maze(&[
        "--command",
        "solve",
        "--input",
        &saved_arg,
        "--output",
        &drawn_arg,
        "--json",
    ]);
    assert_eq!(code(&solved), Some(0), "stderr: {}", stderr(&solved));
    let report = stdout(&solved);
    for field in [
        source_field,
        format!("\"output\":{}", json_string(&drawn_arg)),
    ] {
        assert!(report.contains(&field), "{field} missing from {report}");
    }
}

#[test]
fn the_svg_extension_writes_a_vector_drawing() {
    let dir = Scratch::new("svg");
    let path = dir.path("maze.svg");
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
    let dir = Scratch::new("format");
    let path = dir.path("maze.svg");
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

/// `--format` without `--output` used to be ignored, so `generate --format
/// svg > maze.svg` saved the ASCII rendering into a file named for a drawing.
#[test]
fn format_without_output_writes_that_format_to_stdout() {
    let svg = maze(&[
        "--command",
        "generate",
        "--width",
        "5",
        "--height",
        "4",
        "--format",
        "svg",
        "--quiet",
    ]);
    assert_eq!(code(&svg), Some(0), "stderr: {}", stderr(&svg));
    assert!(stdout(&svg).starts_with("<svg "), "{}", stdout(&svg));
    assert!(stdout(&svg).trim_end().ends_with("</svg>"));

    // Text means the loadable document, exactly what `--output -` writes.
    let args = [
        "--command",
        "generate",
        "--width",
        "5",
        "--height",
        "4",
        "--seed",
        "8",
        "--quiet",
    ];
    let text = maze(&[&args[..], &["--format", "text"]].concat());
    let dash = maze(&[&args[..], &["--output", "-"]].concat());
    assert_eq!(code(&text), Some(0), "stderr: {}", stderr(&text));
    assert_eq!(stdout(&text), stdout(&dash));
    assert!(Map::from_ascii(&stdout(&text)).is_ok());

    let dir = Scratch::new("format-solve");
    let path = write_maze(&dir, "maze", "prim", 6, 5, 2);
    let solved = maze(&[
        "--command",
        "solve",
        "--input",
        &to(&path),
        "--format",
        "svg",
        "--quiet",
    ]);
    assert_eq!(code(&solved), Some(0), "stderr: {}", stderr(&solved));
    assert!(stdout(&solved).starts_with("<svg "), "{}", stdout(&solved));
    assert!(
        stdout(&solved).contains("<polyline "),
        "{}",
        stdout(&solved)
    );
}

#[test]
fn format_without_output_cannot_share_stdout_with_json() {
    let dir = Scratch::new("format-json");
    let path = write_maze(&dir, "maze", "prim", 6, 5, 2);
    let output = maze(&[
        "--command",
        "solve",
        "--input",
        &to(&path),
        "--format",
        "svg",
        "--json",
    ]);
    assert_eq!(code(&output), Some(1));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(stderr(&output).contains("--json"), "{}", stderr(&output));
}

/// The marked-up rendering `solve` writes as text ended without a newline,
/// unlike every other document the tool writes. The plain maze ends the same
/// way, so what is written is also held against the library's rendering of
/// the route, to show it is the marked-up one.
#[test]
fn a_solved_maze_written_as_text_ends_with_a_newline() {
    let dir = Scratch::new("solve-text");
    let path = write_maze(&dir, "maze", "eller", 7, 4, 3);
    let piped = maze(&[
        "--command",
        "solve",
        "--input",
        &to(&path),
        "--output",
        "-",
        "--format",
        "text",
        "--quiet",
    ]);
    assert_eq!(code(&piped), Some(0), "stderr: {}", stderr(&piped));
    assert!(stdout(&piped).ends_with("+\n"), "{:?}", stdout(&piped));
    let (map, route) = library_solution(&path);
    assert_eq!(
        stdout(&piped),
        format!("{}\n", map.to_ascii_with_path(&route)),
        "the maze should be written with its route marked"
    );

    let file = dir.path("solved.txt");
    let saved = maze(&[
        "--command",
        "solve",
        "--input",
        &to(&path),
        "--output",
        &to(&file),
        "--quiet",
    ]);
    assert_eq!(code(&saved), Some(0), "stderr: {}", stderr(&saved));
    let text = fs::read_to_string(&file).expect("the solved maze should exist");
    assert!(text.ends_with("+\n"), "{text:?}");
    assert_eq!(text, stdout(&piped));
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
    let dir = Scratch::new("random");
    let run = |name: &str| {
        let path = dir.path(&format!("{name}.maze"));
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
        fs::read_to_string(dir.path("first.maze")).expect("the first maze"),
        fs::read_to_string(dir.path("second.maze")).expect("the replay"),
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
        &to(&dir.path("reported.maze")),
    ]);
    let notes = stderr(&announced);
    assert!(
        ALGORITHMS
            .iter()
            .any(|name| notes.contains(&format!("'{name}' algorithm"))),
        "the chosen algorithm should be reported: {notes}"
    );
}

/// The algorithm `--algorithm random` picks for a seed, read off the
/// progress line.
fn random_pick(seed: u64) -> (String, String) {
    let output = maze(&[
        "--command",
        "generate",
        "--algorithm",
        "random",
        "--seed",
        &seed.to_string(),
        "--width",
        "4",
        "--height",
        "4",
        "--output",
        "-",
    ]);
    assert_eq!(code(&output), Some(0), "stderr: {}", stderr(&output));
    let notes = stderr(&output);
    let picked = notes
        .split("using the '")
        .nth(1)
        .and_then(|rest| rest.split('\'').next())
        .unwrap_or_else(|| panic!("no algorithm was announced: {notes}"));
    (picked.to_string(), stdout(&output))
}

/// The pick and the maze used to come from the same draw: the pick was the
/// first number of `Rng::new(seed)` and so was the algorithm's first move,
/// so every backtracking maze chosen this way started by carving east from
/// the entrance. Over a fixed run of seeds the two must now look unrelated.
#[test]
fn the_random_pick_does_not_decide_the_first_move() {
    let (mut picked, mut opened_east) = (0, 0);
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..200 {
        let (algorithm, document) = random_pick(seed);
        seen.insert(algorithm.clone());
        if algorithm != "backtracking" {
            continue;
        }
        picked += 1;
        // Line 4 of the document is the first row of cells; the character
        // after the first cell is the wall east of (0, 0).
        let first_row = document.lines().nth(3).expect("a first row");
        if first_row.as_bytes()[3] == b' ' {
            opened_east += 1;
        }
    }
    assert!(
        picked >= 20,
        "too few backtracking picks to judge: {picked}"
    );
    assert!(
        opened_east > 0 && opened_east < picked,
        "the pick still decides the first move: {opened_east} of {picked} opened east"
    );
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        {
            let mut all = ALGORITHMS.to_vec();
            all.sort_unstable();
            all
        },
        "every algorithm should be picked for some seed"
    );
}

/// What a seed picks is part of what it reproduces, so it is pinned.
#[test]
fn a_seed_always_picks_the_same_algorithm() {
    for (seed, expected) in [
        (0, "wilson"),
        (1, "aldous_broder"),
        (2, "prim"),
        (3, "kruskal"),
        (11, "backtracking"),
        (12, "eller"),
    ] {
        assert_eq!(random_pick(seed).0, expected, "seed {seed}");
    }
}

/// A random walk needs about the square of the longer side, so a legal
/// 10x2500000 Aldous-Broder maze ran for about 44 hours. It is refused at once
/// now, with the way out in the message.
#[test]
fn a_random_walk_refuses_a_long_thin_maze_at_once() {
    for algorithm in ["aldous_broder", "wilson"] {
        let output = maze(&[
            "--command",
            "generate",
            "--algorithm",
            algorithm,
            "--width",
            "10",
            "--height",
            "2500000",
            "--quiet",
        ]);
        assert_eq!(code(&output), Some(1), "{algorithm}: {}", stderr(&output));
        let notes = stderr(&output);
        assert!(notes.contains(algorithm), "{notes}");
        assert!(
            notes.contains("backtracking"),
            "the way out should be named: {notes}"
        );
        assert!(stdout(&output).is_empty());
    }
}

/// `--algorithm random` only picks an algorithm that can build the shape, so
/// a long thin maze is never handed to a random walk that would refuse it.
#[test]
fn random_never_picks_an_algorithm_that_would_refuse_the_shape() {
    for seed in 0..24 {
        let output = maze(&[
            "--command",
            "generate",
            "--algorithm",
            "random",
            "--width",
            "2",
            "--height",
            "30000",
            "--seed",
            &seed.to_string(),
            "--output",
            "-",
        ]);
        assert_eq!(code(&output), Some(0), "seed {seed}: {}", stderr(&output));
        let notes = stderr(&output);
        assert!(
            !notes.contains("'aldous_broder'") && !notes.contains("'wilson'"),
            "seed {seed} picked a random walk: {notes}"
        );
    }
}

/// `--input /dev/zero` used to read until memory ran out: 17 GB in two
/// seconds. Reading now stops one byte past the largest document the tool
/// makes, which main.rs puts at twelve bytes a cell plus 4096, so the tool
/// closes its end of a longer stream before the stream is done. The stream
/// here is finite, so losing the limit fails this test instead of exhausting
/// memory.
#[test]
fn an_input_larger_than_any_maze_is_refused_unread() {
    let limit = 12 * maze::generator::MAX_CELLS + 4096;
    let mut child = Command::new(env!("CARGO_BIN_EXE_maze"))
        .args(["--command", "check", "--input", "-", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the maze binary should be runnable");
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let chunk = vec![b'\n'; 1 << 20];
    // Far more than a pipe buffers past the limit, so a tool that keeps
    // reading takes it all and one that stopped leaves the writer failing.
    let mut left = limit + (64 << 20);
    let mut cut_short = false;
    while left > 0 {
        let size = left.min(chunk.len() as u64) as usize;
        if stdin.write_all(&chunk[..size]).is_err() {
            cut_short = true;
            break;
        }
        left -= size as u64;
    }
    drop(stdin);
    let output = child.wait_with_output().expect("the binary should exit");
    assert_eq!(code(&output), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("more than"), "{}", stderr(&output));
    assert!(
        cut_short,
        "the tool read the whole stream instead of stopping at the limit"
    );
}

/// `eprintln!` panics when stderr is a closed pipe, which is what `maze
/// --command check --input m.maze 2>&1 | head -1` does once `head` has its
/// line. The commentary is dropped instead, and the verdict stands.
#[test]
fn a_closed_stderr_does_not_make_the_tool_panic() {
    let scratch = Scratch::new("closed-stderr");
    let path = write_maze(&scratch, "maze", "prim", 6, 5, 2);
    let mut child = Command::new(env!("CARGO_BIN_EXE_maze"))
        .args(["--command", "check", "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the maze binary should be runnable");
    // The "Loaded" note is written once stdin ends, so it always meets the
    // closed pipe.
    drop(child.stderr.take());
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(fs::read_to_string(&path).expect("the fixture").as_bytes())
        .expect("the maze should be accepted on stdin");
    let output = child.wait_with_output().expect("the binary should exit");
    assert_eq!(code(&output), Some(0), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("Solvable: true"),
        "{}",
        stdout(&output)
    );
}

/// A real write error, unlike a reader that stopped early, is a failure:
/// a full disk behind a redirect must not pass for a written maze.
#[cfg(target_os = "linux")]
#[test]
fn a_full_disk_behind_stdout_fails_the_run() {
    let full = fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("Linux has /dev/full");
    let output = Command::new(env!("CARGO_BIN_EXE_maze"))
        .args(["--command", "list", "--quiet"])
        .stdout(Stdio::from(full))
        .output()
        .expect("the maze binary should be runnable");
    assert_eq!(code(&output), Some(1), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("error: stdout:"),
        "{}",
        stderr(&output)
    );
}

/// A failed write names the file it was for; it used to be a bare 'No such
/// file or directory', which for `solve` pointed at the input instead.
#[test]
fn a_failed_solve_write_names_the_file() {
    let scratch = Scratch::new("solve-unwritable");
    let path = write_maze(&scratch, "maze", "prim", 6, 5, 3);
    let target = scratch.path("missing").join("solved.txt");
    let output = maze(&[
        "--command",
        "solve",
        "--input",
        &to(&path),
        "--output",
        &to(&target),
        "--quiet",
    ]);
    assert_eq!(code(&output), Some(1), "{}", stderr(&output));
    assert!(
        stderr(&output).contains(&to(&target)),
        "the error should name {}: {}",
        to(&target),
        stderr(&output)
    );
}

/// The guard used to compare names, so the same maze arriving on stdin, or
/// as `/dev/stdin`, still had its file replaced by the drawing.
#[cfg(unix)]
#[test]
fn solve_refuses_to_overwrite_the_maze_it_reads_from_stdin() {
    let scratch = Scratch::new("stdin-overwrite");
    let path = write_maze(&scratch, "maze", "kruskal", 7, 5, 4);
    let before = fs::read(&path).expect("the fixture");
    for input in ["-", "/dev/stdin"] {
        let output = Command::new(env!("CARGO_BIN_EXE_maze"))
            .args([
                "--command",
                "solve",
                "--input",
                input,
                "--output",
                &to(&path),
                "--quiet",
            ])
            .stdin(Stdio::from(fs::File::open(&path).expect("the fixture")))
            .output()
            .expect("the maze binary should be runnable");
        assert_eq!(code(&output), Some(1), "{input}: {}", stderr(&output));
        assert!(
            stderr(&output).contains("--input"),
            "{input}: {}",
            stderr(&output)
        );
        assert_eq!(
            fs::read(&path).expect("the maze"),
            before,
            "{input} replaced the maze"
        );
    }
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

/// clap's own exit status for bad usage is 2, while the README promises 1
/// for anything that went wrong.
#[test]
fn bad_usage_exits_one_like_every_other_failure() {
    for args in [
        &["--command", "generate", "--width", "abc"][..],
        &["--command", "generate", "--width", "-3"],
        &["--command", "generate", "--format", "bmp"],
        &["--no-such-flag"],
    ] {
        let output = maze(args);
        assert_eq!(code(&output), Some(1), "{args:?}: {}", stderr(&output));
        assert!(!stderr(&output).is_empty(), "{args:?} should say why");
        assert!(stdout(&output).is_empty(), "{args:?}: {}", stdout(&output));
    }
}

#[test]
fn help_and_version_are_successes() {
    let help = maze(&["--help"]);
    assert_eq!(code(&help), Some(0));
    assert!(stdout(&help).contains("--command"), "{}", stdout(&help));

    let version = maze(&["--version"]);
    assert_eq!(code(&version), Some(0));
    assert!(
        stdout(&version).contains(env!("CARGO_PKG_VERSION")),
        "{}",
        stdout(&version)
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

/// `maze --input m.maze --strict` used to print the help and exit 0, so a
/// check whose `--command` had been left out passed every maze. Options
/// without a command are a mistake now, refused before anything is read or
/// written; only a bare `maze` asks for the help.
#[test]
fn options_without_a_command_fail_instead_of_printing_the_help() {
    let dir = Scratch::new("no-command");
    let input = to(&write_maze(&dir, "maze", "backtracking", 5, 5, 1));
    let made = to(&dir.path("made.maze"));
    for args in [
        &["--input", &input, "--strict"][..],
        &["--json", "--input", &input],
        &["--width", "5", "--height", "5", "--output", &made],
        &["--quiet"],
    ] {
        let output = maze(args);
        assert_eq!(
            code(&output),
            Some(1),
            "{args:?} should fail: {}",
            stdout(&output)
        );
        let notes = stderr(&output);
        assert!(
            notes.contains("error:") && notes.contains("--command"),
            "{args:?} should ask for --command: {notes}"
        );
        assert!(
            output.stdout.is_empty(),
            "{args:?} should print nothing, the help included: {}",
            stdout(&output)
        );
        assert_eq!(dir.listing(), ["maze.maze"], "{args:?} created a file");
    }
}

/// The commands share one flat set of options, and each used to ignore the
/// ones it does not read without a word: `generate --input saved.maze` built
/// a brand-new random maze, and `check --output report.txt` exited 0 and
/// wrote nothing. Every other part of each run here is valid, so the option
/// is the only thing that can fail it.
#[test]
fn an_option_the_command_does_not_read_is_refused() {
    let dir = Scratch::new("unused-option");
    let input = to(&write_maze(&dir, "maze", "prim", 5, 4, 6));
    let report = to(&dir.path("report.txt"));
    let cases: [(&str, &str, &[&str]); 7] = [
        (
            "generate",
            "--input",
            &["--input", &input, "--width", "3", "--height", "3"],
        ),
        (
            "check",
            "--output",
            &["--input", &input, "--output", &report],
        ),
        ("check", "--format", &["--input", &input, "--format", "svg"]),
        ("solve", "--seed", &["--input", &input, "--seed", "5"]),
        ("solve", "--braid", &["--input", &input, "--braid"]),
        ("list", "--strict", &["--strict"]),
        ("list", "--input", &["--input", &input]),
    ];
    for (command, option, rest) in cases {
        let output = maze(&[&["--command", command][..], rest].concat());
        assert_eq!(
            code(&output),
            Some(1),
            "{command} {rest:?} should be refused: {}",
            stdout(&output)
        );
        let notes = stderr(&output);
        assert!(
            notes.contains(option) && notes.contains(command),
            "the refusal should name {option} and {command}: {notes}"
        );
        assert!(
            output.stdout.is_empty(),
            "{command} {rest:?} printed: {}",
            stdout(&output)
        );
        assert_eq!(
            dir.listing(),
            ["maze.maze"],
            "{command} {rest:?} wrote a file"
        );
    }
}

/// The other side of the refusal: a command given every option it reads, all
/// at once, still runs, so no option is missing from the list of the
/// commands that read it.
#[test]
fn a_command_accepts_every_option_it_reads_at_once() {
    let dir = Scratch::new("every-option");
    let saved = to(&dir.path("all.maze"));
    let generated = maze(&[
        "--command",
        "generate",
        "--width",
        "6",
        "--height",
        "5",
        "--algorithm",
        "prim",
        "--seed",
        "17",
        "--braid",
        "--entrance-exit",
        "--output",
        &saved,
        "--format",
        "text",
        "--json",
        "--quiet",
    ]);
    assert_eq!(code(&generated), Some(0), "stderr: {}", stderr(&generated));
    let report = stdout(&generated);
    for field in [
        "\"algorithm\":\"prim\"",
        "\"seed\":17",
        "\"width\":6",
        "\"height\":5",
        "\"braid\":true",
        "\"entrance_exit\":true",
        "\"format\":\"text\"",
    ] {
        assert!(report.contains(field), "{field} missing from {report}");
    }

    // A braided maze fails --strict on its own merits, so check and solve are
    // given a perfect one.
    let perfect = to(&write_maze(&dir, "perfect", "eller", 6, 5, 4));
    let drawing = to(&dir.path("solved.svg"));
    for args in [
        &[
            "--command",
            "check",
            "--input",
            &perfect,
            "--strict",
            "--json",
            "--quiet",
        ][..],
        &[
            "--command",
            "solve",
            "--input",
            &perfect,
            "--output",
            &drawing,
            "--format",
            "svg",
            "--json",
            "--quiet",
        ],
        &["--command", "list", "--json", "--quiet"],
    ] {
        let output = maze(args);
        assert_eq!(code(&output), Some(0), "{args:?}: {}", stderr(&output));
    }
}

/// Usage is checked before any work. `generate --json --output -` used to
/// build the whole maze before refusing to share stdout, and `solve` only
/// checked once the maze was solved: a sealed maze printed its JSON verdict
/// and failed for having no solution, and the usage mistake was never
/// reported. A zero width is a mistake the generator reports, so getting the
/// `--json` message for it shows that the flags were checked first.
#[test]
fn usage_is_checked_before_any_work() {
    let dir = Scratch::new("usage-first");
    let sealed = to(&write_sealed(&dir));
    for (args, names) in [
        (
            &[
                "--command",
                "generate",
                "--width",
                "0",
                "--json",
                "--output",
                "-",
            ][..],
            "stdout",
        ),
        (
            &["--command", "generate", "--width", "0", "--json"],
            "--output",
        ),
        (
            &[
                "--command",
                "solve",
                "--input",
                &sealed,
                "--json",
                "--output",
                "-",
            ],
            "stdout",
        ),
        (
            &[
                "--command",
                "solve",
                "--input",
                &sealed,
                "--json",
                "--format",
                "svg",
            ],
            "stdout",
        ),
    ] {
        let output = maze(args);
        assert_eq!(code(&output), Some(1), "{args:?}: {}", stdout(&output));
        let notes = stderr(&output);
        assert!(
            notes.contains("--json") && notes.contains(names),
            "{args:?} should be refused for its flags: {notes}"
        );
        assert!(
            !notes.contains("positive width") && !notes.contains("no solution"),
            "{args:?} did the work before checking the flags: {notes}"
        );
        assert!(
            output.stdout.is_empty(),
            "{args:?} printed: {}",
            stdout(&output)
        );
    }
}
