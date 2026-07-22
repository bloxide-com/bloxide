// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the `cargo-blox list-transitions` command.
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

/// The minimal blox.toml fixture used across tests, with three transitions.
const FIXTURE_THREE_TRANSITIONS: &str = "\
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

[[topology.transitions]]
state = \"Idle\"
event = \"TestMsg::Ping(_)\"
target = \"Active\"
actions = [\"handle_ping\"]

[[topology.transitions]]
state = \"Active\"
event = \"TestMsg::Pong(_)\"
target = \"Active\"
actions = [\"handle_pong\", \"forward_ping\"]

[[topology.transitions]]
state = \"Active\"
event = \"TestMsg::Done(_)\"
target = \"Idle\"

[[topology.transitions.guards]]
condition = \"ctx.round > 5\"
target = \"Idle\"
";

/// A fixture with a transition that has a `feature` key.
const FIXTURE_WITH_FEATURE: &str = "\
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

[[topology.transitions]]
state = \"Idle\"
event = \"TestMsg::Ping(_)\"
target = \"Active\"
actions = [\"handle_ping\"]
feature = \"dynamic\"
";

/// A fixture with a topology section but no transitions.
const FIXTURE_NO_TRANSITIONS: &str = "\
[actor]
name = \"Test\"

[topology]

[[topology.states]]
name = \"Idle\"
initial = true
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

/// Runs `cargo-blox blox list-transitions <blox_name> [--json]` with `cwd` set
/// to `dir` and returns the captured stdout as a string.
fn run_list_transitions(dir: &TempDir, blox_name: &str, json: bool) -> (String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox").arg("list-transitions").arg(blox_name);
    if json {
        cmd.arg("--json");
    }
    let output = cmd.output().expect("spawn cargo-blox");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let success = output.status.success();
    (stdout, success)
}

#[test]
fn list_three_transitions_table() {
    let dir = write_fixture("testblox", FIXTURE_THREE_TRANSITIONS);
    let (stdout, success) = run_list_transitions(&dir, "testblox", false);
    assert!(success, "command should succeed: stderr check needed");
    assert!(stdout.contains("STATE"), "header STATE should be present");
    assert!(stdout.contains("EVENT"), "header EVENT should be present");
    assert!(stdout.contains("TARGET"), "header TARGET should be present");
    assert!(
        stdout.contains("ACTIONS"),
        "header ACTIONS should be present"
    );
    assert!(stdout.contains("GUARDS"), "header GUARDS should be present");
    assert!(
        stdout.contains("FEATURE"),
        "header FEATURE should be present"
    );
    assert!(stdout.contains("Idle"), "state Idle should be listed");
    assert!(stdout.contains("Active"), "state Active should be listed");
    assert!(
        stdout.contains("TestMsg::Ping(_)"),
        "event TestMsg::Ping(_) should be listed"
    );
    assert!(
        stdout.contains("TestMsg::Pong(_)"),
        "event TestMsg::Pong(_) should be listed"
    );
    assert!(
        stdout.contains("TestMsg::Done(_)"),
        "event TestMsg::Done(_) should be listed"
    );
    // Actions should be comma-joined.
    assert!(
        stdout.contains("handle_ping"),
        "action handle_ping should be listed"
    );
    assert!(
        stdout.contains("handle_pong, forward_ping"),
        "actions should be comma-joined"
    );
    // The transition with no actions should show "—".
    assert!(
        stdout.contains('—'),
        "no-action transition should show em-dash"
    );
}

#[test]
fn list_three_transitions_json() {
    let dir = write_fixture("testblox", FIXTURE_THREE_TRANSITIONS);
    let (stdout, success) = run_list_transitions(&dir, "testblox", true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert_eq!(parsed.len(), 3, "should have 3 transitions");

    let states: Vec<&str> = parsed
        .iter()
        .map(|v| v["state"].as_str().expect("state is string"))
        .collect();
    assert_eq!(states, vec!["Idle", "Active", "Active"]);

    let events: Vec<&str> = parsed
        .iter()
        .map(|v| v["event"].as_str().expect("event is string"))
        .collect();
    assert_eq!(
        events,
        vec!["TestMsg::Ping(_)", "TestMsg::Pong(_)", "TestMsg::Done(_)"]
    );

    let targets: Vec<&str> = parsed
        .iter()
        .map(|v| v["target"].as_str().expect("target is string"))
        .collect();
    assert_eq!(targets, vec!["Active", "Active", "Idle"]);

    // First transition: actions = ["handle_ping"]
    let first = &parsed[0];
    let actions: Vec<&str> = first["actions"]
        .as_array()
        .expect("actions is array")
        .iter()
        .map(|v| v.as_str().expect("action is string"))
        .collect();
    assert_eq!(actions, vec!["handle_ping"]);
    assert!(
        first["feature"].is_null(),
        "first transition feature should be null"
    );

    // Second transition: actions = ["handle_pong", "forward_ping"]
    let second = &parsed[1];
    let actions2: Vec<&str> = second["actions"]
        .as_array()
        .expect("actions is array")
        .iter()
        .map(|v| v.as_str().expect("action is string"))
        .collect();
    assert_eq!(actions2, vec!["handle_pong", "forward_ping"]);

    // Third transition: no actions, has guards
    let third = &parsed[2];
    assert!(
        third["actions"].as_array().unwrap().is_empty(),
        "third transition has no actions"
    );
    let guards = third["guards"].as_array().expect("guards is array");
    assert_eq!(guards.len(), 1, "third transition has 1 guard");
    assert_eq!(guards[0]["condition"], "ctx.round > 5");
    assert_eq!(guards[0]["target"], "Idle");
}

#[test]
fn list_empty_transitions_table() {
    let dir = write_fixture("testblox", FIXTURE_NO_TRANSITIONS);
    let (stdout, success) = run_list_transitions(&dir, "testblox", false);
    assert!(success, "command should succeed");
    // Header should be printed even with no transitions.
    assert!(stdout.contains("STATE"), "header should be present");
    // No transition events should appear since there are no transitions.
    assert!(
        !stdout.contains("TestMsg"),
        "no transition events should be present"
    );
}

#[test]
fn list_empty_transitions_json() {
    let dir = write_fixture("testblox", FIXTURE_NO_TRANSITIONS);
    let (stdout, success) = run_list_transitions(&dir, "testblox", true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert!(parsed.is_empty(), "should be empty array");
}

#[test]
fn list_transitions_with_guards_table() {
    let dir = write_fixture("testblox", FIXTURE_THREE_TRANSITIONS);
    let (stdout, success) = run_list_transitions(&dir, "testblox", false);
    assert!(success, "command should succeed");
    // The third transition (Active -> Idle) has 1 guard.
    // Find the line with "TestMsg::Done(_)" and check it has guard count 1.
    let done_line = stdout
        .lines()
        .find(|l| l.contains("TestMsg::Done(_)"))
        .expect("Done transition line present");
    // The GUARDS column should show "1" for this row.
    // Column layout: STATE EVENT TARGET ACTIONS GUARDS FEATURE
    // We just check the line contains the guard count somewhere.
    assert!(
        done_line.contains(" 1 "),
        "guard count 1 should appear in the Done transition row: {done_line}"
    );
    // The other two transitions have 0 guards.
    let ping_line = stdout
        .lines()
        .find(|l| l.contains("TestMsg::Ping(_)"))
        .expect("Ping transition line present");
    assert!(
        ping_line.contains(" 0 "),
        "guard count 0 should appear in the Ping transition row: {ping_line}"
    );
}

#[test]
fn list_transitions_with_guards_json() {
    let dir = write_fixture("testblox", FIXTURE_THREE_TRANSITIONS);
    let (stdout, success) = run_list_transitions(&dir, "testblox", true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    // Third transition has guards.
    let third = &parsed[2];
    let guards = third["guards"].as_array().expect("guards is array");
    assert_eq!(guards.len(), 1, "third transition has 1 guard");
    assert_eq!(guards[0]["condition"], "ctx.round > 5");
    assert_eq!(guards[0]["target"], "Idle");
    // First two transitions have no guards.
    assert!(
        parsed[0]["guards"].as_array().unwrap().is_empty(),
        "first transition has no guards"
    );
    assert!(
        parsed[1]["guards"].as_array().unwrap().is_empty(),
        "second transition has no guards"
    );
}

#[test]
fn list_transitions_with_feature_table() {
    let dir = write_fixture("testblox", FIXTURE_WITH_FEATURE);
    let (stdout, success) = run_list_transitions(&dir, "testblox", false);
    assert!(success, "command should succeed");
    assert!(
        stdout.contains("dynamic"),
        "feature value 'dynamic' should appear in table"
    );
}

#[test]
fn list_transitions_with_feature_json() {
    let dir = write_fixture("testblox", FIXTURE_WITH_FEATURE);
    let (stdout, success) = run_list_transitions(&dir, "testblox", true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert_eq!(parsed.len(), 1, "should have 1 transition");
    assert_eq!(parsed[0]["feature"], "dynamic");
}

#[test]
fn list_transitions_blox_not_found() {
    let dir = TempDir::new().expect("create temp dir");
    let (stdout, success) = run_list_transitions(&dir, "nonexistent", false);
    assert!(!success, "command should fail for missing blox");
    assert!(
        stdout.is_empty() || !stdout.contains("STATE"),
        "no table on error"
    );
}

#[test]
fn list_transitions_blox_not_found_json() {
    let dir = TempDir::new().expect("create temp dir");
    let (_stdout, success) = run_list_transitions(&dir, "nonexistent", true);
    assert!(!success, "command should fail for missing blox");
}
