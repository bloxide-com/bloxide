// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the `cargo-blox list-messages` command.
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

/// The minimal blox.toml fixture used across tests, with three variants.
const FIXTURE_THREE_VARIANTS: &str = "\
[[messages]]
name = \"TestMsg\"
visibility = \"pub\"
copy = true

[[messages.variants]]
name = \"Ping\"

[[messages.variants.fields]]
name = \"round\"
ty = \"u32\"

[[messages.variants]]
name = \"Pong\"

[[messages.variants.fields]]
name = \"round\"
ty = \"u32\"

[[messages.variants]]
name = \"Resume\"
";

/// A fixture with a [[messages]] section but no variants.
const FIXTURE_NO_VARIANTS: &str = "\
[[messages]]
name = \"TestMsg\"
visibility = \"pub\"
copy = true
";

/// Writes a fixture to `<temp>/crates/messages/<crate_name>/blox.toml` and
/// returns the temp dir (kept alive for the duration of the test).
fn write_fixture(crate_name: &str, content: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let msg_dir = dir.path().join("crates/messages").join(crate_name);
    fs::create_dir_all(&msg_dir).expect("create messages dir");
    fs::write(msg_dir.join("blox.toml"), content).expect("write blox.toml");
    dir
}

/// Runs `cargo-blox blox list-messages <crate_name> [--json]` with `cwd` set
/// to `dir` and returns the captured stdout as a string.
fn run_list_messages(dir: &TempDir, crate_name: &str, json: bool) -> (String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox").arg("list-messages").arg(crate_name);
    if json {
        cmd.arg("--json");
    }
    let output = cmd.output().expect("spawn cargo-blox");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let success = output.status.success();
    (stdout, success)
}

#[test]
fn list_three_variants_table() {
    let dir = write_fixture("testmsg", FIXTURE_THREE_VARIANTS);
    let (stdout, success) = run_list_messages(&dir, "testmsg", false);
    assert!(success, "command should succeed");
    assert!(
        stdout.contains("VARIANT"),
        "header VARIANT should be present"
    );
    assert!(stdout.contains("FIELDS"), "header FIELDS should be present");
    assert!(stdout.contains("Ping"), "variant Ping should be listed");
    assert!(stdout.contains("Pong"), "variant Pong should be listed");
    assert!(stdout.contains("Resume"), "variant Resume should be listed");
    // Ping should have the field "round: u32".
    let ping_line = stdout
        .lines()
        .find(|l| l.contains("Ping"))
        .expect("Ping line present");
    assert!(
        ping_line.contains("round: u32"),
        "Ping should show field 'round: u32': {ping_line}"
    );
    // Pong should have the field "round: u32".
    let pong_line = stdout
        .lines()
        .find(|l| l.contains("Pong"))
        .expect("Pong line present");
    assert!(
        pong_line.contains("round: u32"),
        "Pong should show field 'round: u32': {pong_line}"
    );
    // Resume has no fields, should show "(none)".
    let resume_line = stdout
        .lines()
        .find(|l| l.contains("Resume"))
        .expect("Resume line present");
    assert!(
        resume_line.contains("(none)"),
        "Resume should show '(none)': {resume_line}"
    );
}

#[test]
fn list_three_variants_json() {
    let dir = write_fixture("testmsg", FIXTURE_THREE_VARIANTS);
    let (stdout, success) = run_list_messages(&dir, "testmsg", true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert_eq!(parsed.len(), 3, "should have 3 variants");

    let names: Vec<&str> = parsed
        .iter()
        .map(|v| v["name"].as_str().expect("name is string"))
        .collect();
    assert_eq!(names, vec!["Ping", "Pong", "Resume"]);

    // Ping: fields = [{"name": "round", "ty": "u32"}]
    let ping = &parsed[0];
    assert_eq!(ping["name"], "Ping");
    let fields = ping["fields"].as_array().expect("fields is array");
    assert_eq!(fields.len(), 1, "Ping has 1 field");
    assert_eq!(fields[0]["name"], "round");
    assert_eq!(fields[0]["ty"], "u32");

    // Pong: fields = [{"name": "round", "ty": "u32"}]
    let pong = &parsed[1];
    assert_eq!(pong["name"], "Pong");
    let fields = pong["fields"].as_array().expect("fields is array");
    assert_eq!(fields.len(), 1, "Pong has 1 field");
    assert_eq!(fields[0]["name"], "round");
    assert_eq!(fields[0]["ty"], "u32");

    // Resume: fields = []
    let resume = &parsed[2];
    assert_eq!(resume["name"], "Resume");
    assert!(
        resume["fields"].as_array().unwrap().is_empty(),
        "Resume has no fields"
    );
}

#[test]
fn list_empty_variants_table() {
    let dir = write_fixture("testmsg", FIXTURE_NO_VARIANTS);
    let (stdout, success) = run_list_messages(&dir, "testmsg", false);
    assert!(success, "command should succeed");
    // Header should be printed even with no variants.
    assert!(stdout.contains("VARIANT"), "header should be present");
    // No variant names should appear.
    assert!(
        !stdout.contains("Ping"),
        "no variant rows should be present"
    );
}

#[test]
fn list_empty_variants_json() {
    let dir = write_fixture("testmsg", FIXTURE_NO_VARIANTS);
    let (stdout, success) = run_list_messages(&dir, "testmsg", true);
    assert!(success, "command should succeed");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("output should be valid JSON array");
    assert!(parsed.is_empty(), "should be empty array");
}

#[test]
fn list_messages_crate_not_found() {
    let dir = TempDir::new().expect("create temp dir");
    let (stdout, success) = run_list_messages(&dir, "nonexistent", false);
    assert!(!success, "command should fail for missing crate");
    assert!(
        stdout.is_empty() || !stdout.contains("VARIANT"),
        "no table on error"
    );
}

#[test]
fn list_messages_crate_not_found_json() {
    let dir = TempDir::new().expect("create temp dir");
    let (_stdout, success) = run_list_messages(&dir, "nonexistent", true);
    assert!(!success, "command should fail for missing crate");
}
