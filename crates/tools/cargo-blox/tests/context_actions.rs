// Copyright 2025 Bloxide, all rights reserved
//! Library-level integration tests for the `cargo-blox add-action` command's
//! schema fields: `--returns` (only "ActionResult" is recognized — mirrors
//! the codegen/lint hard rule) and `--event-payload` (the extracted payload
//! variable name, written as the `event_payload` key).
//!
//! The cargo-blox crate is a binary, so the command modules are included
//! directly via `#[path]` and the command functions are called in-process.
//! Because path resolution anchors on the current working directory, each
//! test chdirs into its fixture workspace under a process-wide lock.

// The #[path]-included modules bring in more than these tests use.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use tempfile::TempDir;

#[path = "../src/context_cmd.rs"]
mod context_cmd;
#[path = "../src/exit.rs"]
mod exit;
#[path = "../src/toml_helpers.rs"]
mod toml_helpers;
#[path = "../src/utils.rs"]
mod utils;

/// Serializes tests that mutate the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

/// Holds the CWD lock and restores the previous working directory on drop.
struct CwdGuard {
    _lock: MutexGuard<'static, ()>,
    prev: PathBuf,
}

impl CwdGuard {
    fn new(dir: &Path) -> Self {
        let lock = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(dir).expect("chdir to fixture");
        Self { _lock: lock, prev }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.prev);
    }
}

/// Minimal blox.toml fixture: a [context] section with no actions yet.
const FIXTURE: &str = r#"
# Copyright 2025 Bloxide, all rights reserved
# Hand-maintained manifest — comments must survive edits.

[actor]
name = "Worker"

[context]
name = "WorkerCtx"
"#;

/// Writes the fixture to `<temp>/bloxes/<blox_name>/blox.toml` plus
/// a workspace-root Cargo.toml, and returns the temp dir.
fn write_fixture(blox_name: &str, content: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("write workspace Cargo.toml");
    let blox_dir = dir.path().join("bloxes").join(blox_name);
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), content).expect("write blox.toml");
    dir
}

/// Reads back the fixture blox.toml as a raw string.
fn read_back(dir: &TempDir, blox_name: &str) -> String {
    let path = dir.path().join("bloxes").join(blox_name).join("blox.toml");
    fs::read_to_string(&path).expect("read blox.toml back")
}

/// Deserializes every `[[context.actions]]` entry through the codegen
/// schema — proves the (possibly mutated) document still matches
/// `ContextActionConfig` (`deny_unknown_fields`).
fn parse_actions(content: &str) -> Vec<bloxide_codegen::schema::ContextActionConfig> {
    let doc: toml::Value = toml::from_str(content).expect("parse blox.toml back");
    doc.get("context")
        .and_then(|c| c.get("actions"))
        .cloned()
        .map(|v| {
            v.try_into()
                .expect("actions entries must match the ContextActionConfig schema")
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// add-action: --returns ActionResult is written through to the entry.
// ---------------------------------------------------------------------------

#[test]
fn add_action_writes_returns_action_result() {
    let dir = write_fixture("worker", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::add_action(
        "worker",
        "process_work",
        vec!["self_id".to_string()],
        Some("blox_ctx_pool_ref"),
        None,
        None,
        None,
        false,
        Some("ActionResult"),
        None,
        false,
    )
    .expect("add_action should succeed");

    let content = read_back(&dir, "worker");
    assert!(
        content.contains("returns = \"ActionResult\""),
        "the returns key must be written:\n{content}"
    );
    let actions = parse_actions(&content);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].name, "process_work");
    assert_eq!(actions[0].returns.as_deref(), Some("ActionResult"));
}

// ---------------------------------------------------------------------------
// add-action: any other --returns value is rejected (mirroring the
// codegen/lint hard rule) and leaves the file untouched.
// ---------------------------------------------------------------------------

#[test]
fn add_action_rejects_invalid_returns() {
    let dir = write_fixture("worker", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    let err = context_cmd::add_action(
        "worker",
        "process_work",
        vec![],
        Some("blox_ctx_pool_ref"),
        None,
        None,
        None,
        false,
        Some("Result"),
        None,
        false,
    )
    .expect_err("a returns value other than ActionResult must fail");
    assert!(
        err.to_string()
            .contains("the only recognized value is \"ActionResult\""),
        "error must mirror the codegen/lint hard rule: {err:#}"
    );

    // The rejected add must not have modified the file.
    let content = read_back(&dir, "worker");
    assert_eq!(parse_actions(&content).len(), 0);
}

// ---------------------------------------------------------------------------
// add-action: --event-payload is written as the event_payload key; without
// --returns no returns key is emitted.
// ---------------------------------------------------------------------------

#[test]
fn add_action_writes_event_payload() {
    let dir = write_fixture("worker", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::add_action(
        "worker",
        "process_work",
        vec!["self_id".to_string(), "pool_ref:ref".to_string()],
        Some("blox_ctx_pool_ref"),
        None,
        None,
        Some("do_work"),
        false,
        None,
        None,
        false,
    )
    .expect("add_action should succeed");

    let content = read_back(&dir, "worker");
    let actions = parse_actions(&content);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].event_payload.as_deref(), Some("do_work"));
    assert!(
        actions[0].returns.is_none(),
        "no returns key without --returns:\n{content}"
    );
}
