// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the `cargo-blox add-transition` command.
//!
//! Each test creates a temporary directory with a minimal blox.toml fixture,
//! spawns the `cargo-blox` binary as a subprocess, and then reads back the
//! blox.toml to verify the transition was added correctly.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

/// The minimal blox.toml fixture used across tests, with a pre-existing
/// transition (Idle + TestMsg::Ping(_) -> Active) for duplicate tests.
const FIXTURE_BASE: &str = "\
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

/// Runs `cargo-blox blox add-transition <blox_name> [args...]` with `cwd` set
/// to `dir` and returns (stdout, stderr, success).
fn run_add_transition(dir: &TempDir, blox_name: &str, args: &[&str]) -> (String, String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox").arg("add-transition").arg(blox_name);
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().expect("spawn cargo-blox");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let success = output.status.success();
    (stdout, stderr, success)
}

/// Reads back the blox.toml from the temp dir and parses it as a `toml::Value`.
fn read_back_toml(dir: &TempDir, blox_name: &str) -> toml::Value {
    let path = dir
        .path()
        .join("crates/bloxes")
        .join(blox_name)
        .join("blox.toml");
    let content = fs::read_to_string(&path).expect("read blox.toml back");
    toml::from_str(&content).expect("parse blox.toml back")
}

/// Finds the transition matching the given state+event pair in a parsed TOML
/// document. Returns the transition table as a `toml::Value`.
fn find_transition<'a>(doc: &'a toml::Value, state: &str, event: &str) -> &'a toml::Value {
    let topology = doc
        .get("topology")
        .and_then(|t| t.as_table())
        .expect("topology table exists");
    let transitions = topology
        .get("transitions")
        .and_then(|t| t.as_array())
        .expect("transitions array exists");
    transitions
        .iter()
        .find(|t| {
            let tbl = t.as_table().expect("transition is table");
            tbl.get("state").and_then(|v| v.as_str()) == Some(state)
                && tbl.get("event").and_then(|v| v.as_str()) == Some(event)
        })
        .unwrap_or_else(|| panic!("transition {} + {} not found", state, event))
}

// ---------------------------------------------------------------------------
// Test 1: Add basic transition — verify TOML has new transition with state,
// event, target.
// ---------------------------------------------------------------------------

#[test]
fn add_basic_transition() {
    let dir = write_fixture("testblox", FIXTURE_BASE);
    let (stdout, _stderr, success) = run_add_transition(
        &dir,
        "testblox",
        &[
            "--state",
            "Idle",
            "--event",
            "TestMsg::Pong(_)",
            "--target",
            "Active",
        ],
    );
    assert!(success, "command should succeed");
    assert!(
        stdout.contains("Added transition Idle + TestMsg::Pong(_) -> Active to testblox"),
        "stdout should contain success message: {stdout}"
    );

    let doc = read_back_toml(&dir, "testblox");
    let t = find_transition(&doc, "Idle", "TestMsg::Pong(_)");
    let tbl = t.as_table().expect("transition is table");
    assert_eq!(tbl.get("state").and_then(|v| v.as_str()), Some("Idle"));
    assert_eq!(
        tbl.get("event").and_then(|v| v.as_str()),
        Some("TestMsg::Pong(_)")
    );
    assert_eq!(tbl.get("target").and_then(|v| v.as_str()), Some("Active"));
}

// ---------------------------------------------------------------------------
// Test 2: Add with actions — verify `actions` array in TOML.
// ---------------------------------------------------------------------------

#[test]
fn add_with_actions() {
    let dir = write_fixture("testblox", FIXTURE_BASE);
    let (_stdout, _stderr, success) = run_add_transition(
        &dir,
        "testblox",
        &[
            "--state",
            "Idle",
            "--event",
            "TestMsg::Pong(_)",
            "--target",
            "Active",
            "--action",
            "Self::log",
            "--action",
            "Self::forward",
        ],
    );
    assert!(success, "command should succeed");

    let doc = read_back_toml(&dir, "testblox");
    let t = find_transition(&doc, "Idle", "TestMsg::Pong(_)");
    let tbl = t.as_table().expect("transition is table");
    let actions = tbl
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions array exists");
    let action_strs: Vec<&str> = actions
        .iter()
        .map(|v| v.as_str().expect("action is string"))
        .collect();
    assert_eq!(action_strs, vec!["Self::log", "Self::forward"]);
}

// ---------------------------------------------------------------------------
// Test 3: Add with guards — verify `[[topology.transitions.guards]]` in TOML.
// ---------------------------------------------------------------------------

#[test]
fn add_with_guards() {
    let dir = write_fixture("testblox", FIXTURE_BASE);
    let (_stdout, _stderr, success) = run_add_transition(
        &dir,
        "testblox",
        &[
            "--state",
            "Idle",
            "--event",
            "TestMsg::Pong(_)",
            "--target",
            "Active",
            "--guard",
            "ctx.x > 0:Active",
        ],
    );
    assert!(success, "command should succeed");

    let doc = read_back_toml(&dir, "testblox");
    let t = find_transition(&doc, "Idle", "TestMsg::Pong(_)");
    let tbl = t.as_table().expect("transition is table");
    let guards = tbl
        .get("guards")
        .and_then(|v| v.as_array())
        .expect("guards array exists");
    assert_eq!(guards.len(), 1, "should have 1 guard");
    let guard = guards[0].as_table().expect("guard is table");
    assert_eq!(
        guard.get("condition").and_then(|v| v.as_str()),
        Some("ctx.x > 0")
    );
    assert_eq!(guard.get("target").and_then(|v| v.as_str()), Some("Active"));
}

// ---------------------------------------------------------------------------
// Test 4: Add with feature — verify `feature` field in TOML.
// ---------------------------------------------------------------------------

#[test]
fn add_with_feature() {
    let dir = write_fixture("testblox", FIXTURE_BASE);
    let (_stdout, _stderr, success) = run_add_transition(
        &dir,
        "testblox",
        &[
            "--state",
            "Idle",
            "--event",
            "TestMsg::Pong(_)",
            "--target",
            "Active",
            "--feature",
            "dynamic",
        ],
    );
    assert!(success, "command should succeed");

    let doc = read_back_toml(&dir, "testblox");
    let t = find_transition(&doc, "Idle", "TestMsg::Pong(_)");
    let tbl = t.as_table().expect("transition is table");
    assert_eq!(tbl.get("feature").and_then(|v| v.as_str()), Some("dynamic"));
}

// ---------------------------------------------------------------------------
// Test 5: Add with all options — 2 actions, 2 guards, feature — verify all
// fields present.
// ---------------------------------------------------------------------------

#[test]
fn add_with_all_options() {
    let dir = write_fixture("testblox", FIXTURE_BASE);
    let (_stdout, _stderr, success) = run_add_transition(
        &dir,
        "testblox",
        &[
            "--state",
            "Idle",
            "--event",
            "TestMsg::Pong(_)",
            "--target",
            "Active",
            "--action",
            "Self::log",
            "--action",
            "Self::forward",
            "--guard",
            "ctx.x > 0:Active",
            "--guard",
            "ctx.y == 0:Idle",
            "--feature",
            "dynamic",
        ],
    );
    assert!(success, "command should succeed");

    let doc = read_back_toml(&dir, "testblox");
    let t = find_transition(&doc, "Idle", "TestMsg::Pong(_)");
    let tbl = t.as_table().expect("transition is table");

    // Verify state, event, target.
    assert_eq!(tbl.get("state").and_then(|v| v.as_str()), Some("Idle"));
    assert_eq!(
        tbl.get("event").and_then(|v| v.as_str()),
        Some("TestMsg::Pong(_)")
    );
    assert_eq!(tbl.get("target").and_then(|v| v.as_str()), Some("Active"));

    // Verify actions.
    let actions = tbl
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions array exists");
    let action_strs: Vec<&str> = actions
        .iter()
        .map(|v| v.as_str().expect("action is string"))
        .collect();
    assert_eq!(action_strs, vec!["Self::log", "Self::forward"]);

    // Verify guards.
    let guards = tbl
        .get("guards")
        .and_then(|v| v.as_array())
        .expect("guards array exists");
    assert_eq!(guards.len(), 2, "should have 2 guards");
    let g0 = guards[0].as_table().expect("guard 0 is table");
    assert_eq!(
        g0.get("condition").and_then(|v| v.as_str()),
        Some("ctx.x > 0")
    );
    assert_eq!(g0.get("target").and_then(|v| v.as_str()), Some("Active"));
    let g1 = guards[1].as_table().expect("guard 1 is table");
    assert_eq!(
        g1.get("condition").and_then(|v| v.as_str()),
        Some("ctx.y == 0")
    );
    assert_eq!(g1.get("target").and_then(|v| v.as_str()), Some("Idle"));

    // Verify feature.
    assert_eq!(tbl.get("feature").and_then(|v| v.as_str()), Some("dynamic"));
}

// ---------------------------------------------------------------------------
// Test 6: Duplicate rejected — verify command fails (non-zero exit code) when
// state+event pair already exists.
// ---------------------------------------------------------------------------

#[test]
fn duplicate_rejected() {
    let dir = write_fixture("testblox", FIXTURE_BASE);
    // The fixture already has: state=Idle, event=TestMsg::Ping(_), target=Active
    let (_stdout, stderr, success) = run_add_transition(
        &dir,
        "testblox",
        &[
            "--state",
            "Idle",
            "--event",
            "TestMsg::Ping(_)",
            "--target",
            "Done",
        ],
    );
    assert!(!success, "command should fail on duplicate");
    assert!(
        stderr.contains("already exists"),
        "stderr should mention already exists: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Test 7: --if-not-exists on duplicate — verify command succeeds (exit 0)
// and TOML is unchanged.
// ---------------------------------------------------------------------------

#[test]
fn if_not_exists_on_duplicate() {
    let dir = write_fixture("testblox", FIXTURE_BASE);
    // Snapshot the TOML before the command.
    let before = read_back_toml(&dir, "testblox").to_string();

    let (_stdout, _stderr, success) = run_add_transition(
        &dir,
        "testblox",
        &[
            "--state",
            "Idle",
            "--event",
            "TestMsg::Ping(_)",
            "--target",
            "Done",
            "--if-not-exists",
        ],
    );
    assert!(success, "command should succeed with --if-not-exists");

    // TOML should be unchanged.
    let after = read_back_toml(&dir, "testblox").to_string();
    assert_eq!(
        before, after,
        "TOML should be unchanged with --if-not-exists"
    );
}

// ---------------------------------------------------------------------------
// Test 8: Blox not found — verify error (non-zero exit code).
// ---------------------------------------------------------------------------

#[test]
fn blox_not_found() {
    let dir = TempDir::new().expect("create temp dir");
    let (_stdout, _stderr, success) = run_add_transition(
        &dir,
        "nonexistent",
        &[
            "--state",
            "Idle",
            "--event",
            "TestMsg::Pong(_)",
            "--target",
            "Active",
        ],
    );
    assert!(!success, "command should fail for missing blox");
}

// ---------------------------------------------------------------------------
// Test 9: Guard with `::` in condition — verify condition parsed correctly
// (split on last `:`).
// ---------------------------------------------------------------------------

#[test]
fn guard_with_double_colon_in_condition() {
    let dir = write_fixture("testblox", FIXTURE_BASE);
    let (_stdout, _stderr, success) = run_add_transition(
        &dir,
        "testblox",
        &[
            "--state",
            "Idle",
            "--event",
            "TestMsg::Pong(_)",
            "--target",
            "Active",
            "--guard",
            "ctx.msg == TestMsg::Ping(_):Idle",
        ],
    );
    assert!(success, "command should succeed");

    let doc = read_back_toml(&dir, "testblox");
    let t = find_transition(&doc, "Idle", "TestMsg::Pong(_)");
    let tbl = t.as_table().expect("transition is table");
    let guards = tbl
        .get("guards")
        .and_then(|v| v.as_array())
        .expect("guards array exists");
    assert_eq!(guards.len(), 1, "should have 1 guard");
    let guard = guards[0].as_table().expect("guard is table");
    // The condition should contain the `::` from TestMsg::Ping(_), and the
    // target should be "Idle" (split on the LAST ':').
    assert_eq!(
        guard.get("condition").and_then(|v| v.as_str()),
        Some("ctx.msg == TestMsg::Ping(_)")
    );
    assert_eq!(guard.get("target").and_then(|v| v.as_str()), Some("Idle"));
}

// ---------------------------------------------------------------------------
// Test 10: Missing required arg (no --state) — verify clap exits with error.
// ---------------------------------------------------------------------------

#[test]
fn missing_required_arg() {
    let dir = write_fixture("testblox", FIXTURE_BASE);
    let (_stdout, _stderr, success) = run_add_transition(
        &dir,
        "testblox",
        &["--event", "TestMsg::Pong(_)", "--target", "Active"],
    );
    assert!(!success, "command should fail when --state is missing");
}
