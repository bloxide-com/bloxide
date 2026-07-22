// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the `cargo-blox list-states` command.
//!
//! Each test creates a temporary directory with a minimal blox.toml fixture,
//! spawns the `cargo-blox` binary as a subprocess (so that stdout can be
//! captured), and asserts on the printed output.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

/// The minimal blox.toml fixture used across tests, with three states.
const FIXTURE_THREE_STATES: &str = "\
[actor]
name = \"Test\"

[[messages]]
name = \"TestMsg\"
visibility = \"pub\"
copy = true

[[messages.variants]]
name = \"Ping\"

[[messages.variants.fields]]
name = \"round\"
ty = \"u32\"

[topology]

[[topology.states]]
name = \"Idle\"
initial = true

[[topology.states]]
name = \"Active\"

[[topology.states]]
name = \"Done\"
terminal = true
";

/// A fixture with a topology section but no states.
const FIXTURE_NO_STATES: &str = "\
[actor]
name = \"Test\"

[topology]
";

/// Writes a fixture to `<temp>/crates/bloxes/<blox_name>/blox.toml` and returns
/// the temp dir (kept alive for the duration of the test).
fn write_fixture(blox_name: &str, content: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let blox_dir = dir.path().join("crates/bloxes").join(blox_name);
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), content).expect("write blox.toml");
    dir
}

/// Runs `cargo-blox blox list-states <blox_name> [--json]` with `cwd` set to
/// `dir` and returns the captured stdout as a string.
fn run_list_states(dir: &TempDir, blox_name: &str, json: bool) -> (String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox").arg("list-states").arg(blox_name);
    if json {
        cmd.arg("--json");
    }
    let output = cmd.output().expect("spawn cargo-blox");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let success = output.status.success();
    (stdout, success)
}

#[test]
fn list_three_states_table() {
    let dir = write_fixture("testblox", FIXTURE_THREE_STATES);
    let (stdout, success) = run_list_states(&dir, "testblox", false);
    assert!(success, "command should succeed: stderr check needed");
    assert!(stdout.contains("NAME"), "header NAME should be present");
    assert!(
        stdout.contains("INITIAL"),
        "header INITIAL should be present"
    );
    assert!(
        stdout.contains("COMPOSITE"),
        "header COMPOSITE should be present"
    );
    assert!(
        stdout.contains("TERMINAL"),
        "header TERMINAL should be present"
    );
    assert!(stdout.contains("ERROR"), "header ERROR should be present");
    assert!(stdout.contains("PARENT"), "header PARENT should be present");
    assert!(stdout.contains("Idle"), "state Idle should be listed");
    assert!(stdout.contains("Active"), "state Active should be listed");
    assert!(stdout.contains("Done"), "state Done should be listed");
    // The initial state Idle should have `true` in the INITIAL column.
    let idle_line = stdout
        .lines()
        .find(|l| l.contains("Idle"))
        .expect("Idle line present");
    assert!(idle_line.contains("true"), "Idle should be initial=true");
}

#[test]
fn list_three_states_json() {
    let dir = write_fixture("testblox", FIXTURE_THREE_STATES);
    let (stdout, success) = run_list_states(&dir, "testblox", true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert_eq!(parsed.len(), 3, "should have 3 states");
    let names: Vec<&str> = parsed
        .iter()
        .map(|v| v["name"].as_str().expect("name is string"))
        .collect();
    assert_eq!(names, vec!["Idle", "Active", "Done"]);

    let idle = &parsed[0];
    assert_eq!(idle["name"], "Idle");
    assert_eq!(idle["initial"], true);
    assert_eq!(idle["composite"], false);
    assert_eq!(idle["terminal"], false);
    assert_eq!(idle["error"], false);
    assert!(idle["parent"].is_null(), "Idle parent should be null");

    let done = &parsed[2];
    assert_eq!(done["name"], "Done");
    assert_eq!(done["terminal"], true);
    assert_eq!(done["initial"], false);
}

#[test]
fn list_empty_states_table() {
    let dir = write_fixture("testblox", FIXTURE_NO_STATES);
    let (stdout, success) = run_list_states(&dir, "testblox", false);
    assert!(success, "command should succeed");
    // Header should be printed even with no states.
    assert!(stdout.contains("NAME"), "header should be present");
    // No state names beyond the header.
    assert!(!stdout.contains("Idle"), "no state rows should be present");
}

#[test]
fn list_empty_states_json() {
    let dir = write_fixture("testblox", FIXTURE_NO_STATES);
    let (stdout, success) = run_list_states(&dir, "testblox", true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert!(parsed.is_empty(), "should be empty array");
}

#[test]
fn list_blox_not_found() {
    let dir = TempDir::new().expect("create temp dir");
    let (stdout, success) = run_list_states(&dir, "nonexistent", false);
    assert!(!success, "command should fail for missing blox");
    assert!(
        stdout.is_empty() || !stdout.contains("NAME"),
        "no table on error"
    );
}

#[test]
fn list_blox_not_found_json() {
    let dir = TempDir::new().expect("create temp dir");
    let (_stdout, success) = run_list_states(&dir, "nonexistent", true);
    assert!(!success, "command should fail for missing blox");
}
