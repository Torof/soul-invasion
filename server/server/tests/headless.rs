//! Runs the real server binary: the scenario check that the CI will grow
//! over time (D09, "accelerated mode without players is mandatory from
//! the start").

use std::process::Command;

fn server() -> Command {
    Command::new(env!("CARGO_BIN_EXE_server"))
}

#[test]
fn headless_run_simulates_the_requested_days_and_exits_cleanly() {
    let output = server()
        .args([
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
        stdout.contains("done: 1800 ticks, 3 events, now day 4"),
        "stdout: {stdout}"
    );
}

#[test]
fn paced_run_with_a_day_limit_stops_on_its_own() {
    // 2 ticks/s, 1 s/day, 2 days, 100x faster: finishes in a few milliseconds.
    let output = server()
        .args([
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
        stdout.contains("stopped: 4 ticks, 2 events, now day 3"),
        "stdout: {stdout}"
    );
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
