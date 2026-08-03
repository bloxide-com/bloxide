// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the semantic process exit codes (spec 19):
//! 0 success, 1 other, 2 usage (clap), 3 not found, 5 conflict.
//!
//! `exit::code_of` is exercised directly (the module is included via
//! `#[path]` since cargo-blox is a binary crate), and the full path through
//! `main` → `dispatch` → `exit_process` is exercised by spawning the binary
//! and asserting on its process exit status.

// The #[path]-included module brings in more than these tests use.
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

#[path = "../src/exit.rs"]
mod exit;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

// ---------------------------------------------------------------------------
// Library level: code_of maps CodedError / EditError by downcasting.
// ---------------------------------------------------------------------------

#[test]
fn code_of_coded_errors() {
    let err: anyhow::Error = exit::conflict("already exists").into();
    assert_eq!(exit::code_of(&err), Some(exit::EXIT_CONFLICT));

    let err: anyhow::Error = exit::not_found("no such thing").into();
    assert_eq!(exit::code_of(&err), Some(exit::EXIT_NOT_FOUND));
}

#[test]
fn code_of_edit_errors() {
    use bloxide_codegen::edit::EditError;

    let err: anyhow::Error = EditError::Conflict("duplicate".to_string()).into();
    assert_eq!(exit::code_of(&err), Some(exit::EXIT_CONFLICT));

    let err: anyhow::Error = EditError::NotFound("missing".to_string()).into();
    assert_eq!(exit::code_of(&err), Some(exit::EXIT_NOT_FOUND));
}

#[test]
fn code_of_uncoded_error_is_none() {
    let err = anyhow::anyhow!("plain failure");
    assert_eq!(exit::code_of(&err), None);
}

#[test]
fn code_of_sees_through_anyhow_context() {
    // Commands wrap coded errors in anyhow layers; downcasting (not string
    // matching) must still recover the code.
    let err: anyhow::Error = exit::not_found("missing variant").into();
    let err = err.context("while editing blox.toml");
    assert_eq!(exit::code_of(&err), Some(exit::EXIT_NOT_FOUND));
}

// ---------------------------------------------------------------------------
// Process level: the binary exits with the semantic code.
// ---------------------------------------------------------------------------

const MESSAGES_FIXTURE: &str = "\
[[messages]]
name = \"TestMsg\"

[[messages.variants]]
name = \"Ping\"
";

const SYSTEM_FIXTURE: &str = "\
[system]
runtime = \"tokio\"
";

/// Writes a workspace fixture with a messages crate and an app.
fn write_fixture() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let msg_dir = dir.path().join("crates/messages/test-messages");
    fs::create_dir_all(&msg_dir).expect("create messages dir");
    fs::write(msg_dir.join("blox.toml"), MESSAGES_FIXTURE).expect("write blox.toml");
    let app_dir = dir.path().join("apps/demo");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::write(app_dir.join("system.toml"), SYSTEM_FIXTURE).expect("write system.toml");
    dir
}

/// Runs `cargo-blox blox <args...>` in `dir` and returns the process exit code.
fn run_blox(dir: &TempDir, args: &[&str]) -> i32 {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox");
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().expect("spawn cargo-blox");
    output.status.code().expect("exit code present")
}

#[test]
fn not_found_exits_3() {
    let dir = write_fixture();
    // Removing a variant that does not exist → not found.
    let code = run_blox(&dir, &["remove-message", "test-messages", "Nope"]);
    assert_eq!(code, 3, "not-found errors must exit 3");
}

#[test]
fn conflict_exits_5() {
    let dir = write_fixture();
    // Adding a variant that already exists → conflict.
    let code = run_blox(&dir, &["add-message", "test-messages", "Ping"]);
    assert_eq!(code, 5, "conflict errors must exit 5");
}

#[test]
fn other_errors_exit_1() {
    let dir = write_fixture();
    // Unknown supervision strategy → plain (uncoded) error.
    let code = run_blox(
        &dir,
        &[
            "add-supervision",
            "demo",
            "--supervisor",
            "sup",
            "--strategy",
            "one_for_one",
        ],
    );
    assert_eq!(code, 1, "uncoded errors must exit 1");
}

#[test]
fn usage_errors_exit_2() {
    let dir = write_fixture();
    // Missing required positional arg → clap usage error.
    let code = run_blox(&dir, &["add-message"]);
    assert_eq!(code, 2, "clap usage errors must exit 2");
}

#[test]
fn success_exits_0() {
    let dir = write_fixture();
    let code = run_blox(&dir, &["add-message", "test-messages", "Pong"]);
    assert_eq!(code, 0, "success must exit 0");
}
