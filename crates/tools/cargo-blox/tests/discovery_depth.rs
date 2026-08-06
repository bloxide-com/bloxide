// Copyright 2025 Bloxide, all rights reserved
//! Integration test for workspace discovery depth: `cargo blox generate` and
//! `cargo blox lint` must discover the SAME set of blox.toml files, including
//! one nested one level deeper than the standard layout. Regression test for
//! the walk-depth drift where generate used `max_depth(4)` and lint
//! `max_depth(5)` — a blox.toml at walkdir depth 5 was linted but never
//! generated.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

/// Minimal valid blox.toml (no topology → no diagnostics). The `[[messages]]`
/// section makes codegen emit one file: manifests that produce no files at
/// all (e.g. bloxide-core's `[mailboxes]`-only blox.toml, consumed by its
/// build.rs) are skipped by generate and would not exercise discovery.
const BLOX_FIXTURE: &str = "\
[actor]
name = \"Foo\"

[[messages]]
name = \"FooMsg\"

[[messages.variants]]
name = \"Ping\"
";

/// Writes a fixture workspace with a blox.toml at the standard layout depth
/// and one nested one level deeper:
///   Cargo.toml ([workspace] + [workspace.dependencies])
///   bloxes/foo/blox.toml          (walkdir depth 3 — pure-TOML layout)
///   crates/nested/deep/deeper/blox.toml  (walkdir depth 5 — nested in-crate layout)
fn write_nested_fixture() -> TempDir {
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
    let nested_dir = dir.path().join("crates/nested/deep/deeper");
    fs::create_dir_all(&nested_dir).expect("create nested blox dir");
    fs::write(nested_dir.join("blox.toml"), BLOX_FIXTURE).expect("write nested blox.toml");
    dir
}

/// Runs `cargo-blox blox <args...>` in `dir` and returns (stdout, stderr, success).
fn run_blox(dir: &TempDir, args: &[&str]) -> (String, String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
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
fn generate_and_lint_discover_the_same_blox_tomls() {
    let dir = write_nested_fixture();

    let (stdout, _stderr, success) = run_blox(&dir, &["lint"]);
    assert!(success, "lint should succeed");
    assert!(
        stdout.contains("across 2 blox.toml files"),
        "lint should discover the nested blox.toml too: {stdout}"
    );

    let root = dir.path().to_str().expect("utf-8 temp path");
    let (stdout, stderr, success) = run_blox(&dir, &["generate", "--workspace", root]);
    assert!(success, "generate should succeed: {stderr}");
    assert!(
        stdout.contains("processed 2 blox.toml files"),
        "generate must process the same set lint discovered: {stdout}"
    );
    assert!(
        dir.path()
            .join("crates/nested/deep/deeper/src/generated/mod.rs")
            .exists(),
        "generate should have written src/generated for the nested crate"
    );
    assert!(
        dir.path()
            .join("target/bloxide-generated/crates/foo-blox/src/generated/mod.rs")
            .exists(),
        "generate should have materialized the pure-TOML blox crate"
    );
}
