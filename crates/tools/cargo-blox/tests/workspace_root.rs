// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for workspace-root resolution: subcommands must work
//! when run from any subdirectory of the workspace, not just the root.
//!
//! Each test builds a fixture workspace (with a `[workspace]` Cargo.toml at
//! the root), spawns the `cargo-blox` binary with its CWD set to a
//! SUBDIRECTORY, and asserts the command still finds and edits the right
//! file under the root.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

const MESSAGES_FIXTURE: &str = "\
[[messages]]
name = \"TestMsg\"

[[messages.variants]]
name = \"Ping\"
";

const BLOX_FIXTURE: &str = "\
[actor]
name = \"Ws\"

[topology]

[[topology.states]]
name = \"Idle\"
initial = true

[[topology.states]]
name = \"Active\"
";

/// Writes a fixture workspace:
///   Cargo.toml ([workspace])
///   crates/messages/test-messages/blox.toml
///   crates/bloxes/wsblox/blox.toml
fn write_fixture() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("write workspace Cargo.toml");
    let msg_dir = dir.path().join("crates/messages/test-messages");
    fs::create_dir_all(&msg_dir).expect("create messages dir");
    fs::write(msg_dir.join("blox.toml"), MESSAGES_FIXTURE).expect("write messages blox.toml");
    let blox_dir = dir.path().join("crates/bloxes/wsblox");
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), BLOX_FIXTURE).expect("write blox blox.toml");
    dir
}

/// Runs `cargo-blox blox <args...>` with `cwd` set to `cwd` and returns
/// (stdout, stderr, success).
fn run_blox_in(cwd: &PathBuf, args: &[&str]) -> (String, String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(cwd);
    cmd.arg("blox");
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().expect("spawn cargo-blox");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

#[test]
fn list_messages_from_crate_subdirectory() {
    let dir = write_fixture();
    let cwd = dir.path().join("crates/messages/test-messages");
    let (stdout, stderr, success) = run_blox_in(&cwd, &["list-messages", "test-messages"]);
    assert!(
        success,
        "list-messages from a subdirectory should succeed: {stderr}"
    );
    assert!(
        stdout.contains("Ping"),
        "variant Ping should be listed: {stdout}"
    );
}

#[test]
fn list_states_from_crate_subdirectory() {
    let dir = write_fixture();
    let cwd = dir.path().join("crates/bloxes/wsblox");
    let (stdout, stderr, success) = run_blox_in(&cwd, &["list-states", "wsblox"]);
    assert!(
        success,
        "list-states from a subdirectory should succeed: {stderr}"
    );
    assert!(
        stdout.contains("Idle"),
        "state Idle should be listed: {stdout}"
    );
    assert!(
        stdout.contains("Active"),
        "state Active should be listed: {stdout}"
    );
}

#[test]
fn add_message_from_nested_subdirectory_edits_workspace_file() {
    let dir = write_fixture();
    // A nested subdirectory that is NOT the crate dir.
    let cwd = dir.path().join("crates/messages");
    let (_stdout, stderr, success) = run_blox_in(&cwd, &["add-message", "test-messages", "Pong"]);
    assert!(
        success,
        "add-message from a subdirectory should succeed: {stderr}"
    );

    // The file under the workspace ROOT (not ./crates/messages relative to
    // the subdir) must have been edited.
    let content = fs::read_to_string(dir.path().join("crates/messages/test-messages/blox.toml"))
        .expect("read blox.toml back");
    assert!(
        content.contains("Pong"),
        "Pong should be added to the workspace-root file:\n{content}"
    );
}
