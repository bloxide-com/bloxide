// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for IDE integration: `cargo blox generate` emits
//! `.vscode/settings.json` declaring `rust-analyzer.linkedProjects` for both
//! the root manifest and the generated workspace
//! (target/bloxide-generated/Cargo.toml), so a fresh clone needs only
//! `cargo blox generate` before rust-analyzer indexes both workspaces.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

/// Minimal valid blox.toml (same fixture as discovery_depth.rs) — enough for
/// lint to pass and for generate to materialize the pure-TOML crate into
/// target/bloxide-generated/.
const BLOX_FIXTURE: &str = "\
[actor]
name = \"Foo\"

[[messages]]
name = \"FooMsg\"

[[messages.variants]]
name = \"Ping\"
";

/// Writes a fixture workspace with one pure-TOML blox:
///   Cargo.toml ([workspace] + [workspace.dependencies])
///   bloxes/foo/blox.toml
fn write_fixture() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    // bloxes/foo is a pure-TOML source: the materializer resolves the
    // derived bloxide-core dependency via [workspace.dependencies].
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\n\n[workspace.dependencies]\nbloxide-core = { path = \"crates/bloxide-core\" }\n",
    )
    .expect("write workspace Cargo.toml");
    let blox_dir = dir.path().join("bloxes/foo");
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), BLOX_FIXTURE).expect("write blox.toml");
    dir
}

/// Runs `cargo-blox blox generate` against the fixture root and returns
/// (stdout, stderr, success).
fn run_generate(dir: &TempDir) -> (String, String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox");
    cmd.args(["generate", "--workspace"]);
    cmd.arg(dir.path());
    let output = cmd.output().expect("spawn cargo-blox");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

#[test]
fn generate_emits_vscode_settings_with_linked_projects() {
    let dir = write_fixture();
    let (stdout, stderr, success) = run_generate(&dir);
    assert!(success, "generate should succeed: {stderr}");

    // The generated workspace manifest the settings point at must exist.
    assert!(
        dir.path()
            .join("target/bloxide-generated/Cargo.toml")
            .exists(),
        "generate should materialize the generated workspace manifest"
    );

    let settings_path = dir.path().join(".vscode/settings.json");
    let content = fs::read_to_string(&settings_path).unwrap_or_else(|_| {
        panic!(".vscode/settings.json should be emitted by generate:\n{stdout}\n{stderr}")
    });
    let settings: serde_json::Value =
        serde_json::from_str(&content).expect("settings.json should be valid JSON");
    assert_eq!(
        settings["rust-analyzer.linkedProjects"],
        serde_json::json!(["./Cargo.toml", "./target/bloxide-generated/Cargo.toml"]),
        "linkedProjects should list the root manifest explicitly (setting it \
         disables auto-discovery) plus the generated workspace manifest:\n{content}"
    );
}

#[test]
fn generate_vscode_settings_is_idempotent() {
    let dir = write_fixture();
    let (_stdout, stderr, success) = run_generate(&dir);
    assert!(success, "first generate should succeed: {stderr}");

    let settings_path = dir.path().join(".vscode/settings.json");
    let first = fs::read_to_string(&settings_path).expect("read settings after first run");

    let (stdout, stderr, success) = run_generate(&dir);
    assert!(success, "second generate should succeed: {stderr}");
    let second = fs::read_to_string(&settings_path).expect("read settings after second run");
    assert_eq!(first, second, "settings.json content must be stable");
    assert!(
        !stdout.contains("settings.json"),
        "second generate must not rewrite .vscode/settings.json (write-if-changed):\n{stdout}"
    );
}

#[test]
fn generate_preserves_unrelated_vscode_settings() {
    let dir = write_fixture();
    let vscode_dir = dir.path().join(".vscode");
    fs::create_dir_all(&vscode_dir).expect("create .vscode dir");
    fs::write(
        vscode_dir.join("settings.json"),
        "{\n  \"editor.tabSize\": 2\n}\n",
    )
    .expect("write pre-existing settings.json");

    let (_stdout, stderr, success) = run_generate(&dir);
    assert!(success, "generate should succeed: {stderr}");

    let content =
        fs::read_to_string(vscode_dir.join("settings.json")).expect("read merged settings.json");
    let settings: serde_json::Value =
        serde_json::from_str(&content).expect("merged settings.json should be valid JSON");
    assert_eq!(
        settings["editor.tabSize"],
        serde_json::json!(2),
        "unrelated user settings must be preserved:\n{content}"
    );
    assert_eq!(
        settings["rust-analyzer.linkedProjects"],
        serde_json::json!(["./Cargo.toml", "./target/bloxide-generated/Cargo.toml"]),
        "linkedProjects must be merged in:\n{content}"
    );
}
