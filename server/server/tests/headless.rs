//! Runs the real server binary: the scenario check that the CI will grow
//! over time (D09, "accelerated mode without players is mandatory from
//! the start").

use std::process::Command;

fn server() -> Command {
    // A small valley: a new world is made in seconds (`--valley-size`).
    let mut command = Command::new(env!("CARGO_BIN_EXE_server"));
    command.args(["--valley-size", "768"]);
    command
}

#[test]
fn headless_run_simulates_the_requested_days_and_exits_cleanly() {
    let output = server()
        .args([
            "--ephemeral",
            "--headless",
            "--days",
            "3",
            "--ticks-per-second",
            "10",
            "--seconds-per-day",
            "60",
        ])
        .output()
        .expect("server binary runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("done: 1800 ticks,") && stdout.contains("now day 4"),
        "stdout: {stdout}"
    );
}

#[test]
fn paced_run_with_a_day_limit_stops_on_its_own() {
    // 2 ticks/s, 1 s/day, 2 days, 100x faster: finishes in a few milliseconds.
    // No client comes, so the plants around the camp are not placed first.
    let output = server()
        .args([
            "--no-warm-up",
            "--ephemeral",
            "--listen",
            "127.0.0.1:0",
            "--days",
            "2",
            "--time-scale",
            "100",
            "--ticks-per-second",
            "2",
            "--seconds-per-day",
            "1",
        ])
        .output()
        .expect("server binary runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("listening on 127.0.0.1:"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("tick 2: DayStarted { day: 2 }"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("stopped: 4 ticks,") && stdout.contains("now day 3"),
        "stdout: {stdout}"
    );
}

#[test]
fn the_world_resumes_from_its_database() {
    let dir = std::env::temp_dir().join(format!("soul-invasion-resume-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let data = dir.join("world.sqlite");
    let data = data.to_str().unwrap();
    let calendar = ["--ticks-per-second", "10", "--seconds-per-day", "60"];

    let first = server()
        .args(["--data", data, "--headless", "--days", "3"])
        .args(calendar)
        .output()
        .expect("server binary runs");
    assert!(first.status.success());
    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(stdout.contains("world: new"), "stdout: {stdout}");
    assert!(stdout.contains("now day 4"), "stdout: {stdout}");

    // Second run: a different calendar on the command line is ignored,
    // the world's own calendar wins, and time continues where it stopped.
    let second = server()
        .args(["--data", data, "--headless", "--days", "2"])
        .args(["--ticks-per-second", "1", "--seconds-per-day", "1"])
        .output()
        .expect("server binary runs");
    assert!(second.status.success());
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(
        stdout.contains("world: resumed at tick 1800 (day 4)"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("done: 1200 ticks,") && stdout.contains("now day 6"),
        "stdout: {stdout}"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_without_days_is_refused() {
    let output = server()
        .arg("--headless")
        .output()
        .expect("server binary runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires `--days <N>`"));
}

#[test]
fn help_is_printed() {
    let output = server().arg("--help").output().expect("server binary runs");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("USAGE"));
}

#[test]
fn reset_erases_the_saved_world_and_starts_anew() {
    let dir = std::env::temp_dir().join(format!("soul-invasion-reset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let data = dir.join("world.sqlite");
    let data = data.to_str().unwrap();
    let calendar = ["--ticks-per-second", "10", "--seconds-per-day", "60"];

    let first = server()
        .args(["--data", data, "--headless", "--days", "2"])
        .args(calendar)
        .output()
        .expect("server binary runs");
    assert!(first.status.success());

    let reset = server()
        .args(["--data", data, "--reset", "--headless", "--days", "1"])
        .args(calendar)
        .output()
        .expect("server binary runs");
    let stdout = String::from_utf8_lossy(&reset.stdout);
    assert!(reset.status.success(), "stdout: {stdout}");
    assert!(
        stdout.contains("(reset: starting a new world)"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("world: new"), "stdout: {stdout}");
    assert!(stdout.contains("now day 2"), "stdout: {stdout}");

    // The reset world was saved: the next run resumes from it, not from day 3.
    let next = server()
        .args(["--data", data, "--headless", "--days", "1"])
        .args(calendar)
        .output()
        .expect("server binary runs");
    let stdout = String::from_utf8_lossy(&next.stdout);
    assert!(
        stdout.contains("world: resumed at tick 600 (day 2)"),
        "stdout: {stdout}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// How a run in [`paced_run`] ends, once the server listens.
#[derive(Clone, Copy, PartialEq)]
enum End {
    /// `SIGTERM`, as scripts stop a server: a clean stop.
    Term,
    /// Killed: a crash, no final checkpoint.
    Crash,
}

/// A paced server on `data`, ended as `end` once it listens. Its output.
fn paced_run(data: &str, extra: &[&str], end: End) -> String {
    use std::io::{BufRead, BufReader};

    let mut child = server()
        .args(["--no-warm-up", "--data", data, "--listen", "127.0.0.1:0"])
        .args(extra)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("server binary runs");
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut told = String::new();
    for line in lines.by_ref() {
        let line = line.unwrap();
        told.push_str(&line);
        told.push('\n');
        if line.starts_with("listening on ") {
            break;
        }
    }
    if end == End::Crash {
        child.kill().unwrap();
        child.wait().unwrap();
        return told;
    }
    let sent = std::process::Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success());
    for line in lines {
        told.push_str(&line.unwrap());
        told.push('\n');
    }
    assert!(child.wait().unwrap().success(), "stdout: {told}");
    assert!(told.contains("stopping (SIGTERM)"), "stdout: {told}");
    told
}

/// The world id the server told in its output.
fn world_id(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("world id: "))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("no world id in: {stdout}"))
        .to_owned()
}

#[test]
fn the_world_id_changes_after_a_crash_or_a_reset_and_only_then() {
    let dir = std::env::temp_dir().join(format!("soul-invasion-epoch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let data = dir.join("world.sqlite");
    let data = data.to_str().unwrap();
    let run = |extra: &[&str], end| world_id(&paced_run(data, extra, end));

    // A clean stop (SIGTERM): the next run is the same world to a client.
    let first = run(&[], End::Term);
    assert_eq!(run(&[], End::Term), first);

    // A crash: killed while running, no final checkpoint, no mark.
    assert_eq!(run(&[], End::Crash), first, "the run before was clean");

    // The run after the crash is another world; clean stops keep it.
    let after = run(&[], End::Term);
    assert_ne!(after, first);
    assert_eq!(run(&[], End::Term), after);

    // A world made anew on the same seed is another world too.
    let reset = run(&["--reset"], End::Term);
    assert!(reset != first && reset != after, "{reset}");
    assert_eq!(run(&[], End::Term), reset);

    let _ = std::fs::remove_dir_all(dir);
}
