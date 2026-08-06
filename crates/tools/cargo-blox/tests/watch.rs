// Copyright 2025 Bloxide, all rights reserved
//! Regression test for `cargo blox watch`: it must watch the user's project
//! — the workspace found by walking up from the invocation directory — NOT
//! `CARGO_MANIFEST_DIR`, which points at the cargo-blox crate itself when
//! the binary runs under `cargo run` (regenerating files inside the tool
//! crate instead of the user's project).
//!
//! The test spawns the binary from a crate subdirectory of a fixture
//! workspace with `CARGO_MANIFEST_DIR` set to a decoy directory, edits a
//! blox.toml, and asserts codegen output appears under the fixture.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

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
///   Cargo.toml ([workspace] + [workspace.dependencies])
///   bloxes/wsblox/blox.toml
fn write_fixture() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    // bloxes/wsblox is a pure-TOML source: the materializer resolves
    // the derived bloxide-core dependency via [workspace.dependencies].
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\n\n[workspace.dependencies]\nbloxide-core = { path = \"crates/bloxide-core\" }\n",
    )
    .expect("write workspace Cargo.toml");
    let blox_dir = dir.path().join("bloxes/wsblox");
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), BLOX_FIXTURE).expect("write blox.toml");
    dir
}

#[test]
fn watch_regenerates_in_invocation_workspace() {
    let dir = write_fixture();
    let blox_dir = dir.path().join("bloxes/wsblox");
    // Decoy: stands in for the cargo-blox crate dir that CARGO_MANIFEST_DIR
    // points at under `cargo run`. watch must ignore it entirely.
    let decoy = TempDir::new().expect("create decoy dir");

    let mut child = Command::new(blox_bin())
        .arg("blox")
        .arg("watch")
        .current_dir(&blox_dir)
        .env("CARGO_MANIFEST_DIR", decoy.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cargo-blox watch");

    // Pure-TOML bloxes are materialized into target/bloxide-generated
    // (crate name = kebab-case actor name + "-blox").
    let generated_mod = dir
        .path()
        .join("target/bloxide-generated/crates/ws-blox/src/generated/mod.rs");
    let blox_toml = blox_dir.join("blox.toml");

    // watch debounces regeneration at 500 ms from startup, so keep editing
    // blox.toml until a change lands outside that window and codegen output
    // appears under the FIXTURE (not the decoy).
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut ticks = 0;
    while Instant::now() < deadline && !generated_mod.exists() {
        std::thread::sleep(Duration::from_millis(700));
        ticks += 1;
        fs::write(&blox_toml, format!("{}# tick {}\n", BLOX_FIXTURE, ticks))
            .expect("touch blox.toml");
    }

    let regenerated = generated_mod.exists();
    child.kill().expect("kill cargo-blox watch");
    child.wait().expect("wait on cargo-blox watch");

    assert!(
        regenerated,
        "watch should regenerate under the invocation workspace, not CARGO_MANIFEST_DIR"
    );
    assert!(
        !decoy.path().join("src").exists(),
        "nothing should be generated inside the CARGO_MANIFEST_DIR decoy"
    );
}
