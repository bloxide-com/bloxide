// Copyright 2025 Bloxide, all rights reserved
//! Library-level integration tests for the `cargo-blox add-use` /
//! `remove-use` commands, covering both `[[context.uses]]` shapes:
//! single-field (`field`/`field_type`/`role`) and multi-field (inline
//! `fields = [...]` and nested `[[context.uses.fields]]` sub-tables, as in
//! pool/blox.toml).
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

/// Pool-like blox.toml fixture: one single-field `[[context.uses]]` entry,
/// one multi-field entry in inline `fields = [...]` form (pool/blox.toml
/// style), and one multi-field entry in nested `[[context.uses.fields]]`
/// sub-table form.
const FIXTURE: &str = r#"
# Copyright 2025 Bloxide, all rights reserved
# Hand-maintained manifest — comments must survive edits.

[actor]
name = "Pool"

[context]
name = "PoolCtx"

# Single-field ctor param.
[[context.uses]]
field = "self_ref"
field_type = "ActorRef<PoolMsg, R>"
role = "ctor"

# Multi-field spawn capability (inline form, like pool/blox.toml).
[[context.uses]]
feature = "dynamic"
fields = [
    { name = "spawn_fn", ty = "SpawnFn<R, SpawnRequest<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>>", role = "ctor" },
    { name = "spawn_ref", ty = "ActorRef<ChildCtrl<R>, R>", role = "ctor" },
    { name = "pending_task_id", ty = "u32", role = "state" },
]

# Multi-field timer capability (nested sub-table form).
[[context.uses]]

  [[context.uses.fields]]
  name = "timer_ref"
  ty = "ActorRef<TimerCommand, R>"
  role = "ctor"

  [[context.uses.fields]]
  name = "current_timer"
  ty = "Option<TimerId>"
  role = "state"
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

/// Deserializes every `[[context.uses]]` entry through the codegen schema —
/// proves the (possibly mutated) document still matches `ContextUse`
/// (`deny_unknown_fields`), for both shapes.
fn parse_uses(content: &str) -> Vec<bloxide_codegen::schema::ContextUse> {
    let doc: toml::Value = toml::from_str(content).expect("parse blox.toml back");
    doc.get("context")
        .and_then(|c| c.get("uses"))
        .cloned()
        .map(|v| {
            v.try_into()
                .expect("uses entries must match the ContextUse schema")
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// remove-use: single-field entries are removed whole, comments survive.
// ---------------------------------------------------------------------------

#[test]
fn remove_use_removes_single_field_entry() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::remove_use("pool", "self_ref").expect("remove_use should succeed");

    let content = read_back(&dir, "pool");
    let uses = parse_uses(&content);
    assert_eq!(uses.len(), 2, "only the two multi-field entries remain");
    assert!(uses.iter().all(|u| u.field.is_none()));
    assert!(
        content.contains("# Multi-field spawn capability (inline form, like pool/blox.toml)."),
        "comments must be preserved:\n{content}"
    );
    assert!(
        content.contains("# Copyright 2025 Bloxide, all rights reserved"),
        "copyright header must be preserved:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// remove-use: a sub-field is removed from an inline multi-field entry; the
// entry itself and its other sub-fields survive.
// ---------------------------------------------------------------------------

#[test]
fn remove_use_removes_inline_sub_field() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::remove_use("pool", "spawn_fn").expect("remove_use should succeed");

    let content = read_back(&dir, "pool");
    let uses = parse_uses(&content);
    assert_eq!(uses.len(), 3, "no whole entry should be removed");
    let spawn = &uses[1];
    assert_eq!(spawn.feature.as_deref(), Some("dynamic"));
    let names: Vec<&str> = spawn.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["spawn_ref", "pending_task_id"]);
}

// ---------------------------------------------------------------------------
// remove-use: a sub-field is removed from a nested [[context.uses.fields]]
// entry as well.
// ---------------------------------------------------------------------------

#[test]
fn remove_use_removes_nested_sub_field() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::remove_use("pool", "timer_ref").expect("remove_use should succeed");

    let content = read_back(&dir, "pool");
    let uses = parse_uses(&content);
    assert_eq!(uses.len(), 3);
    let names: Vec<&str> = uses[2].fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["current_timer"]);
}

// ---------------------------------------------------------------------------
// remove-use: an entry whose last sub-fields are removed is dropped entirely
// (no empty [[context.uses]] husk left behind).
// ---------------------------------------------------------------------------

#[test]
fn remove_use_drops_entry_left_without_sub_fields() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::remove_use("pool", "timer_ref").expect("first removal");
    context_cmd::remove_use("pool", "current_timer").expect("second removal");

    let content = read_back(&dir, "pool");
    let uses = parse_uses(&content);
    assert_eq!(
        uses.len(),
        2,
        "the emptied timer entry must be dropped:\n{content}"
    );
    assert!(
        !content.contains("timer_ref") && !content.contains("current_timer"),
        "no timer fields may remain:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// remove-use: no matching entry or sub-field → loud not-found (exit 3).
// ---------------------------------------------------------------------------

#[test]
fn remove_use_missing_field_is_not_found_coded() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    let err = context_cmd::remove_use("pool", "nope").expect_err("missing field should fail");
    assert_eq!(
        exit::code_of(&err),
        Some(exit::EXIT_NOT_FOUND),
        "remove-use of a missing field must map to exit code 3"
    );

    // The failed remove must not have modified the file.
    let content = read_back(&dir, "pool");
    assert_eq!(parse_uses(&content).len(), 3);
}

// ---------------------------------------------------------------------------
// add-use: single-field shape still works (regression) and round-trips
// through the ContextUse schema.
// ---------------------------------------------------------------------------

#[test]
fn add_use_single_field_shape() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::add_use(
        "pool",
        Some("peer_ref"),
        Some("ActorRef<PoolMsg, R>"),
        Some("ctor"),
        &[],
        None,
        false,
    )
    .expect("add_use should succeed");

    let content = read_back(&dir, "pool");
    let uses = parse_uses(&content);
    assert_eq!(uses.len(), 4);
    let added = &uses[3];
    assert_eq!(added.field.as_deref(), Some("peer_ref"));
    assert_eq!(added.field_type.as_deref(), Some("ActorRef<PoolMsg, R>"));
    assert_eq!(added.role.as_deref(), Some("ctor"));
    assert!(added.fields.is_empty());
}

// ---------------------------------------------------------------------------
// add-use: multi-field shape creates an inline fields = [...] entry whose
// sub-fields deserialize through the ContextUse schema. Types containing
// `::` paths must survive the name:ty:role parsing.
// ---------------------------------------------------------------------------

#[test]
fn add_use_multi_field_shape() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::add_use(
        "pool",
        None,
        None,
        None,
        &[
            "spawn_queue:Vec<u32>:state".to_string(),
            "notify_ref:ActorRef<bloxide_core::lifecycle::ChildLifecycleEvent, R>:ctor".to_string(),
        ],
        Some("dynamic"),
        false,
    )
    .expect("add_use should succeed");

    let content = read_back(&dir, "pool");
    assert!(
        content.contains("fields = ["),
        "multi-field entries are written as an inline array:\n{content}"
    );
    let uses = parse_uses(&content);
    assert_eq!(uses.len(), 4);
    let added = &uses[3];
    assert_eq!(added.feature.as_deref(), Some("dynamic"));
    assert!(added.field.is_none());
    assert_eq!(added.fields.len(), 2);
    assert_eq!(added.fields[0].name, "spawn_queue");
    assert_eq!(added.fields[0].ty, "Vec<u32>");
    assert_eq!(added.fields[0].role.as_deref(), Some("state"));
    assert_eq!(added.fields[1].name, "notify_ref");
    assert_eq!(
        added.fields[1].ty, "ActorRef<bloxide_core::lifecycle::ChildLifecycleEvent, R>",
        "types with `::` paths must survive name:ty:role parsing"
    );
    assert_eq!(added.fields[1].role.as_deref(), Some("ctor"));
}

// ---------------------------------------------------------------------------
// add-use: dedup keys on any contributed field name — a single-field add
// colliding with a multi-field sub-field is a conflict (exit 5), and vice
// versa.
// ---------------------------------------------------------------------------

#[test]
fn add_use_conflicts_against_multi_field_sub_field() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    // Single-field add whose name already exists as a sub-field.
    let err = context_cmd::add_use(
        "pool",
        Some("spawn_fn"),
        Some("bool"),
        Some("state"),
        &[],
        None,
        false,
    )
    .expect_err("duplicate sub-field name should fail");
    assert_eq!(exit::code_of(&err), Some(exit::EXIT_CONFLICT));

    // Multi-field add one of whose names already exists top-level.
    let err = context_cmd::add_use(
        "pool",
        None,
        None,
        None,
        &[
            "brand_new:u32:state".to_string(),
            "self_ref:bool:state".to_string(),
        ],
        None,
        false,
    )
    .expect_err("duplicate top-level field name should fail");
    assert_eq!(exit::code_of(&err), Some(exit::EXIT_CONFLICT));

    // Conflicts must not have modified the file.
    let content = read_back(&dir, "pool");
    assert_eq!(parse_uses(&content).len(), 3);
}

// ---------------------------------------------------------------------------
// add-use: --if-not-exists tolerates a duplicate (of any shape) silently.
// ---------------------------------------------------------------------------

#[test]
fn add_use_if_not_exists_tolerates_duplicate() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::add_use(
        "pool",
        Some("pending_task_id"),
        Some("u32"),
        Some("state"),
        &[],
        None,
        true,
    )
    .expect("--if-not-exists should tolerate the duplicate");

    let content = read_back(&dir, "pool");
    assert_eq!(parse_uses(&content).len(), 3, "file must be unchanged");
}

// ---------------------------------------------------------------------------
// add-use: malformed shapes fail loudly and leave the file untouched.
// ---------------------------------------------------------------------------

#[test]
fn add_use_malformed_shapes_error() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    // Missing role in the sub-field spec.
    assert!(context_cmd::add_use(
        "pool",
        None,
        None,
        None,
        &["spawn_queue:Vec<u32>".to_string()],
        None,
        false,
    )
    .is_err());
    // Unknown role.
    assert!(context_cmd::add_use(
        "pool",
        None,
        None,
        None,
        &["spawn_queue:Vec<u32>:bogus".to_string()],
        None,
        false,
    )
    .is_err());
    // Single-field args must be given together.
    assert!(context_cmd::add_use(
        "pool",
        Some("peer_ref"),
        None,
        Some("ctor"),
        &[],
        None,
        false,
    )
    .is_err());
    // The two shapes are mutually exclusive.
    assert!(context_cmd::add_use(
        "pool",
        Some("peer_ref"),
        Some("bool"),
        Some("ctor"),
        &["spawn_queue:Vec<u32>:state".to_string()],
        None,
        false,
    )
    .is_err());
    // Nothing to add at all.
    assert!(context_cmd::add_use("pool", None, None, None, &[], None, false).is_err());

    let content = read_back(&dir, "pool");
    assert_eq!(parse_uses(&content).len(), 3, "file must be unchanged");
}
