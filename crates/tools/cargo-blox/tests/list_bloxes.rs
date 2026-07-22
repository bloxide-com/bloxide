// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the `cargo-blox list-bloxes` command.
//!
//! Each test creates a temporary directory with blox.toml fixtures under
//! `crates/bloxes/`, spawns the `cargo-blox` binary as a subprocess (so
//! that stdout can be captured), and asserts on the printed output.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

/// Fixture for blox `alpha`: 2 states, 1 transition, 1 message variant.
const FIXTURE_ALPHA: &str = "\
[actor]
name = \"Alpha\"

[[messages]]
name = \"AlphaMsg\"
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

[[topology.transitions]]
state = \"Idle\"
event = \"AlphaMsg::Ping(_)\"
target = \"Active\"
";

/// Fixture for blox `beta`: 3 states, 2 transitions, 2 message variants.
const FIXTURE_BETA: &str = "\
[actor]
name = \"Beta\"

[[messages]]
name = \"BetaMsg\"
visibility = \"pub\"
copy = true

[[messages.variants]]
name = \"Start\"

[[messages.variants.fields]]
name = \"id\"
ty = \"u32\"

[[messages.variants]]
name = \"Stop\"

[topology]

[[topology.states]]
name = \"Idle\"
initial = true

[[topology.states]]
name = \"Running\"

[[topology.states]]
name = \"Done\"
terminal = true

[[topology.transitions]]
state = \"Idle\"
event = \"BetaMsg::Start(_)\"
target = \"Running\"

[[topology.transitions]]
state = \"Running\"
event = \"BetaMsg::Stop\"
target = \"Done\"
";

/// Fixture for blox `gamma`: 1 state, 0 transitions, 1 message variant.
const FIXTURE_GAMMA: &str = "\
[actor]
name = \"Gamma\"

[[messages]]
name = \"GammaMsg\"
visibility = \"pub\"
copy = true

[[messages.variants]]
name = \"Tick\"

[topology]

[[topology.states]]
name = \"Idle\"
initial = true
";

/// Writes a fixture to `<temp>/crates/bloxes/<blox_name>/blox.toml`.
fn write_blox_fixture(dir: &TempDir, blox_name: &str, content: &str) {
    let blox_dir = dir.path().join("crates/bloxes").join(blox_name);
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), content).expect("write blox.toml");
}

/// Creates a temp dir with the three blox fixtures (alpha, beta, gamma).
fn make_three_bloxes() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    write_blox_fixture(&dir, "alpha", FIXTURE_ALPHA);
    write_blox_fixture(&dir, "beta", FIXTURE_BETA);
    write_blox_fixture(&dir, "gamma", FIXTURE_GAMMA);
    dir
}

/// Runs `cargo-blox blox list-bloxes [--json]` with `cwd` set to `dir` and
/// returns the captured stdout as a string and success flag.
fn run_list_bloxes(dir: &TempDir, json: bool) -> (String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox").arg("list-bloxes");
    if json {
        cmd.arg("--json");
    }
    let output = cmd.output().expect("spawn cargo-blox");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let success = output.status.success();
    (stdout, success)
}

#[test]
fn list_three_bloxes_table() {
    let dir = make_three_bloxes();
    let (stdout, success) = run_list_bloxes(&dir, false);
    assert!(success, "command should succeed");
    // Headers should be present.
    assert!(stdout.contains("NAME"), "header NAME should be present");
    assert!(stdout.contains("STATES"), "header STATES should be present");
    assert!(
        stdout.contains("TRANSITIONS"),
        "header TRANSITIONS should be present"
    );
    assert!(
        stdout.contains("MESSAGES"),
        "header MESSAGES should be present"
    );
    // All three blox names should appear, sorted alphabetically.
    let alpha_line = stdout
        .lines()
        .find(|l| l.contains("alpha"))
        .expect("alpha line present");
    let beta_line = stdout
        .lines()
        .find(|l| l.contains("beta"))
        .expect("beta line present");
    let gamma_line = stdout
        .lines()
        .find(|l| l.contains("gamma"))
        .expect("gamma line present");

    // alpha: 2 states, 1 transition, 1 message
    assert!(
        alpha_line.contains("2"),
        "alpha should have 2 states: {alpha_line}"
    );
    assert!(
        alpha_line.contains("1"),
        "alpha should have 1 transition / 1 message: {alpha_line}"
    );

    // beta: 3 states, 2 transitions, 2 messages
    assert!(
        beta_line.contains("3"),
        "beta should have 3 states: {beta_line}"
    );
    // beta line should contain "2" for transitions and messages
    let beta_parts: Vec<&str> = beta_line.split_whitespace().collect();
    assert_eq!(beta_parts.len(), 4, "beta line should have 4 columns");
    assert_eq!(beta_parts[0], "beta");
    assert_eq!(beta_parts[1], "3", "beta states = 3");
    assert_eq!(beta_parts[2], "2", "beta transitions = 2");
    assert_eq!(beta_parts[3], "2", "beta messages = 2");

    // gamma: 1 state, 0 transitions, 1 message
    let gamma_parts: Vec<&str> = gamma_line.split_whitespace().collect();
    assert_eq!(gamma_parts.len(), 4, "gamma line should have 4 columns");
    assert_eq!(gamma_parts[0], "gamma");
    assert_eq!(gamma_parts[1], "1", "gamma states = 1");
    assert_eq!(gamma_parts[2], "0", "gamma transitions = 0");
    assert_eq!(gamma_parts[3], "1", "gamma messages = 1");

    // Verify alphabetical sort: alpha before beta before gamma.
    let alpha_pos = stdout.find("alpha").expect("alpha position");
    let beta_pos = stdout.find("beta").expect("beta position");
    let gamma_pos = stdout.find("gamma").expect("gamma position");
    assert!(alpha_pos < beta_pos, "alpha should come before beta");
    assert!(beta_pos < gamma_pos, "beta should come before gamma");
}

#[test]
fn list_three_bloxes_json() {
    let dir = make_three_bloxes();
    let (stdout, success) = run_list_bloxes(&dir, true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert_eq!(parsed.len(), 3, "should have 3 bloxes");

    // Sorted alphabetically: alpha, beta, gamma
    assert_eq!(parsed[0]["name"], "alpha");
    assert_eq!(parsed[0]["states"], 2);
    assert_eq!(parsed[0]["transitions"], 1);
    assert_eq!(parsed[0]["messages"], 1);

    assert_eq!(parsed[1]["name"], "beta");
    assert_eq!(parsed[1]["states"], 3);
    assert_eq!(parsed[1]["transitions"], 2);
    assert_eq!(parsed[1]["messages"], 2);

    assert_eq!(parsed[2]["name"], "gamma");
    assert_eq!(parsed[2]["states"], 1);
    assert_eq!(parsed[2]["transitions"], 0);
    assert_eq!(parsed[2]["messages"], 1);
}

#[test]
fn list_empty_bloxes_table() {
    let dir = TempDir::new().expect("create temp dir");
    // Create the crates/bloxes directory but with no subdirectories.
    fs::create_dir_all(dir.path().join("crates/bloxes")).expect("create bloxes dir");
    let (stdout, success) = run_list_bloxes(&dir, false);
    assert!(success, "command should succeed");
    // Header should be printed even with no bloxes.
    assert!(stdout.contains("NAME"), "header should be present");
    assert!(stdout.contains("STATES"), "header should be present");
    // No blox data rows.
    assert!(!stdout.contains("alpha"), "no blox rows should be present");
}

#[test]
fn list_empty_bloxes_json() {
    let dir = TempDir::new().expect("create temp dir");
    // Create the crates/bloxes directory but with no subdirectories.
    fs::create_dir_all(dir.path().join("crates/bloxes")).expect("create bloxes dir");
    let (stdout, success) = run_list_bloxes(&dir, true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert!(parsed.is_empty(), "should be empty array");
}

#[test]
fn list_no_bloxes_dir_table() {
    let dir = TempDir::new().expect("create temp dir");
    // Don't create crates/bloxes at all — should still succeed with empty output.
    let (stdout, success) = run_list_bloxes(&dir, false);
    assert!(success, "command should succeed with no bloxes dir");
    assert!(stdout.contains("NAME"), "header should be present");
}

#[test]
fn list_no_bloxes_dir_json() {
    let dir = TempDir::new().expect("create temp dir");
    let (stdout, success) = run_list_bloxes(&dir, true);
    assert!(success, "command should succeed with no bloxes dir");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert!(parsed.is_empty(), "should be empty array");
}
