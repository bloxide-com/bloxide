// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the `cargo-blox remove-transition` command.
//!
//! Each test creates a temporary directory with a minimal blox.toml fixture,
//! spawns the `cargo-blox` binary as a subprocess, and then reads back the
//! blox.toml to verify the transition was removed correctly.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

/// The minimal blox.toml fixture with three transitions. The third
/// transition (`Active + TestMsg::Done(_) -> Done`) carries a guard.
const FIXTURE_THREE: &str = "\
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

[[topology.transitions]]
state = \"Idle\"
event = \"TestMsg::Ping(_)\"
target = \"Active\"

[[topology.transitions]]
state = \"Active\"
event = \"TestMsg::Pong(_)\"
target = \"Active\"
actions = [\"handle_pong\"]

[[topology.transitions]]
state = \"Active\"
event = \"TestMsg::Done(_)\"
target = \"Done\"

[[topology.transitions.guards]]
condition = \"ctx.round > 5\"
target = \"Done\"
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

/// Runs `cargo-blox blox remove-transition <blox_name> --state <s> --event <e>`
/// with `cwd` set to `dir` and returns (stdout, stderr, success).
fn run_remove_transition(
    dir: &TempDir,
    blox_name: &str,
    state: &str,
    event: &str,
) -> (String, String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox")
        .arg("remove-transition")
        .arg(blox_name)
        .arg("--state")
        .arg(state)
        .arg("--event")
        .arg(event);
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

/// Returns the transitions array from a parsed TOML doc, or `None` if absent.
fn transitions_array(doc: &toml::Value) -> Option<&Vec<toml::Value>> {
    doc.get("topology")
        .and_then(|t| t.as_table())
        .and_then(|t| t.get("transitions"))
        .and_then(|t| t.as_array())
}

/// Returns the raw text of the blox.toml in the temp dir (for substring checks).
fn read_back_text(dir: &TempDir, blox_name: &str) -> String {
    let path = dir
        .path()
        .join("crates/bloxes")
        .join(blox_name)
        .join("blox.toml");
    fs::read_to_string(&path).expect("read blox.toml back")
}

// ---------------------------------------------------------------------------
// Test 1: Remove existing transition — verify TOML no longer has it.
// ---------------------------------------------------------------------------

#[test]
fn remove_existing_transition() {
    let dir = write_fixture("testblox", FIXTURE_THREE);
    let (stdout, _stderr, success) =
        run_remove_transition(&dir, "testblox", "Idle", "TestMsg::Ping(_)");
    assert!(success, "command should succeed");
    assert!(
        stdout.contains("Removed transition Idle + TestMsg::Ping(_) from testblox"),
        "stdout should contain success message: {stdout}"
    );

    let doc = read_back_toml(&dir, "testblox");
    let transitions = transitions_array(&doc).expect("transitions array exists");
    let still_present = transitions.iter().any(|t| {
        let tbl = t.as_table().expect("transition is table");
        tbl.get("state").and_then(|v| v.as_str()) == Some("Idle")
            && tbl.get("event").and_then(|v| v.as_str()) == Some("TestMsg::Ping(_)")
    });
    assert!(
        !still_present,
        "transition Idle + TestMsg::Ping(_) should have been removed"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Remove non-existent transition — verify command fails.
// ---------------------------------------------------------------------------

#[test]
fn remove_nonexistent_transition() {
    let dir = write_fixture("testblox", FIXTURE_THREE);
    let (_stdout, stderr, success) =
        run_remove_transition(&dir, "testblox", "Idle", "TestMsg::Nope(_)");
    assert!(!success, "command should fail for missing transition");
    assert!(
        stderr.contains("not found"),
        "stderr should mention not found: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Remove a transition that has guards — verify both the transition
// and its nested guards are gone from the TOML.
// ---------------------------------------------------------------------------

#[test]
fn remove_transition_with_guards() {
    let dir = write_fixture("testblox", FIXTURE_THREE);
    // The third transition (Active + TestMsg::Done(_) -> Done) has a guard
    // with condition "ctx.round > 5" and target "Done".
    let (_stdout, _stderr, success) =
        run_remove_transition(&dir, "testblox", "Active", "TestMsg::Done(_)");
    assert!(success, "command should succeed");

    // The transition should be gone from the parsed TOML.
    let doc = read_back_toml(&dir, "testblox");
    let transitions = transitions_array(&doc).expect("transitions array exists");
    let still_present = transitions.iter().any(|t| {
        let tbl = t.as_table().expect("transition is table");
        tbl.get("state").and_then(|v| v.as_str()) == Some("Active")
            && tbl.get("event").and_then(|v| v.as_str()) == Some("TestMsg::Done(_)")
    });
    assert!(
        !still_present,
        "transition Active + TestMsg::Done(_) should have been removed"
    );

    // The guard condition and target should no longer appear in the raw TOML
    // text (they were nested under the removed transition).
    let text = read_back_text(&dir, "testblox");
    assert!(
        !text.contains("ctx.round > 5"),
        "guard condition should be gone from TOML: {text}"
    );
    // "Done" still appears as a state name, so only check the guard's target
    // is gone by ensuring no `[[topology.transitions.guards]]` block remains.
    assert!(
        !text.contains("[[topology.transitions.guards]]"),
        "guards block should be gone from TOML: {text}"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Remove preserves others — blox with 3 transitions, remove 1,
// verify the other 2 remain.
// ---------------------------------------------------------------------------

#[test]
fn remove_preserves_others() {
    let dir = write_fixture("testblox", FIXTURE_THREE);
    let (_stdout, _stderr, success) =
        run_remove_transition(&dir, "testblox", "Idle", "TestMsg::Ping(_)");
    assert!(success, "command should succeed");

    let doc = read_back_toml(&dir, "testblox");
    let transitions = transitions_array(&doc).expect("transitions array exists");
    // Two should remain.
    assert_eq!(
        transitions.len(),
        2,
        "expected 2 transitions to remain, got {}: {:?}",
        transitions.len(),
        transitions
    );

    // Verify the two remaining transitions are the expected ones.
    let has_pong = transitions.iter().any(|t| {
        let tbl = t.as_table().expect("transition is table");
        tbl.get("state").and_then(|v| v.as_str()) == Some("Active")
            && tbl.get("event").and_then(|v| v.as_str()) == Some("TestMsg::Pong(_)")
    });
    assert!(has_pong, "Active + TestMsg::Pong(_) should remain");
    let has_done = transitions.iter().any(|t| {
        let tbl = t.as_table().expect("transition is table");
        tbl.get("state").and_then(|v| v.as_str()) == Some("Active")
            && tbl.get("event").and_then(|v| v.as_str()) == Some("TestMsg::Done(_)")
    });
    assert!(has_done, "Active + TestMsg::Done(_) should remain");
}

// ---------------------------------------------------------------------------
// Test 5: Blox not found — verify error (non-zero exit code).
// ---------------------------------------------------------------------------

#[test]
fn blox_not_found() {
    let dir = TempDir::new().expect("create temp dir");
    let (_stdout, _stderr, success) =
        run_remove_transition(&dir, "nonexistent", "Idle", "TestMsg::Ping(_)");
    assert!(!success, "command should fail for missing blox");
}

// ---------------------------------------------------------------------------
// Test 6/7: Feature-gated variants — `--feature` targets exactly one of two
// transitions sharing a (state, event) pair.
// ---------------------------------------------------------------------------

/// Fixture with two transitions on the same (state, event) pair: one
/// non-gated, one gated on `feature = "dynamic"`.
const FIXTURE_GATED_PAIR: &str = "\
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

[[topology.transitions]]
state = \"Idle\"
event = \"TestMsg::Ping(_)\"
target = \"stay\"
feature = \"dynamic\"
";

/// Like `run_remove_transition`, with an optional `--feature` argument.
fn run_remove_transition_feature(
    dir: &TempDir,
    blox_name: &str,
    state: &str,
    event: &str,
    feature: Option<&str>,
) -> (String, String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox")
        .arg("remove-transition")
        .arg(blox_name)
        .arg("--state")
        .arg(state)
        .arg("--event")
        .arg(event);
    if let Some(feat) = feature {
        cmd.arg("--feature").arg(feat);
    }
    let output = cmd.output().expect("spawn cargo-blox");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let success = output.status.success();
    (stdout, stderr, success)
}

/// (state, event, feature) triples of the transitions in the read-back TOML.
fn gated_keys(doc: &toml::Value) -> Vec<(String, String, Option<String>)> {
    transitions_array(doc)
        .expect("transitions array exists")
        .iter()
        .map(|t| {
            let tbl = t.as_table().expect("transition is table");
            (
                tbl["state"].as_str().expect("state").to_string(),
                tbl["event"].as_str().expect("event").to_string(),
                tbl.get("feature")
                    .and_then(|f| f.as_str())
                    .map(str::to_string),
            )
        })
        .collect()
}

#[test]
fn remove_feature_gated_variant_leaves_non_gated() {
    let dir = write_fixture("testblox", FIXTURE_GATED_PAIR);
    let (stdout, _stderr, success) = run_remove_transition_feature(
        &dir,
        "testblox",
        "Idle",
        "TestMsg::Ping(_)",
        Some("dynamic"),
    );
    assert!(success, "command should succeed");
    assert!(
        stdout
            .contains("Removed transition Idle + TestMsg::Ping(_) (feature dynamic) from testblox"),
        "stdout should mention the feature gate: {stdout}"
    );

    let doc = read_back_toml(&dir, "testblox");
    assert_eq!(
        gated_keys(&doc),
        vec![("Idle".to_string(), "TestMsg::Ping(_)".to_string(), None)],
        "only the non-gated transition should remain"
    );
}

#[test]
fn remove_without_feature_leaves_gated_variant() {
    let dir = write_fixture("testblox", FIXTURE_GATED_PAIR);
    let (_stdout, _stderr, success) =
        run_remove_transition_feature(&dir, "testblox", "Idle", "TestMsg::Ping(_)", None);
    assert!(success, "command should succeed");

    let doc = read_back_toml(&dir, "testblox");
    assert_eq!(
        gated_keys(&doc),
        vec![(
            "Idle".to_string(),
            "TestMsg::Ping(_)".to_string(),
            Some("dynamic".to_string())
        )],
        "only the feature-gated transition should remain"
    );
}

#[test]
fn remove_missing_feature_variant_fails() {
    let dir = write_fixture("testblox", FIXTURE_GATED_PAIR);
    let (_stdout, stderr, success) = run_remove_transition_feature(
        &dir,
        "testblox",
        "Idle",
        "TestMsg::Ping(_)",
        Some("nonexistent-feature"),
    );
    assert!(
        !success,
        "command should fail for a missing feature variant"
    );
    assert!(
        stderr.contains("not found"),
        "stderr should mention not found: {stderr}"
    );

    // Both transitions untouched.
    let doc = read_back_toml(&dir, "testblox");
    assert_eq!(gated_keys(&doc).len(), 2, "nothing should be removed");
}
