// Copyright 2025 Bloxide, all rights reserved
//! Library-level integration tests for the `cargo-blox remove-field` command,
//! covering every shape a context field can live in: `[[context.fields]]`
//! entries, single-field `[[context.uses]]` entries, and multi-field
//! `[[context.uses]]` sub-fields in both representations (inline
//! `fields = [...]` as in pool/blox.toml, and nested
//! `[[context.uses.fields]]` sub-tables).
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

/// Pool-like blox.toml fixture: `[[context.fields]]` state fields plus one
/// single-field `[[context.uses]]` entry, one multi-field entry in inline
/// `fields = [...]` form (pool/blox.toml style), and one multi-field entry
/// in nested `[[context.uses.fields]]` sub-table form.
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

# Direct context fields.
[[context.fields]]
name = "worker_refs"
type = "Vec<ActorRef<WorkerMsg, R>>"

[[context.fields]]
name = "pending"
type = "u32"

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

/// Deserializes every `[[context.fields]]` entry through the codegen schema.
fn parse_fields(content: &str) -> Vec<bloxide_codegen::schema::ContextFieldConfig> {
    let doc: toml::Value = toml::from_str(content).expect("parse blox.toml back");
    doc.get("context")
        .and_then(|c| c.get("fields"))
        .cloned()
        .map(|v| {
            v.try_into()
                .expect("fields entries must match the ContextFieldConfig schema")
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// remove-field: a [[context.fields]] entry is removed, comments survive.
// ---------------------------------------------------------------------------

#[test]
fn remove_field_removes_context_fields_entry() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::remove_field("pool", "pending").expect("remove_field should succeed");

    let content = read_back(&dir, "pool");
    let fields = parse_fields(&content);
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["worker_refs"]);
    assert_eq!(parse_uses(&content).len(), 3, "uses entries untouched");
    assert!(
        content.contains("# Multi-field spawn capability (inline form, like pool/blox.toml)."),
        "comments must be preserved:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// remove-field: a single-field [[context.uses]] entry is removed whole.
// ---------------------------------------------------------------------------

#[test]
fn remove_field_removes_single_field_use_entry() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::remove_field("pool", "self_ref").expect("remove_field should succeed");

    let content = read_back(&dir, "pool");
    let uses = parse_uses(&content);
    assert_eq!(uses.len(), 2, "only the two multi-field entries remain");
    assert!(uses.iter().all(|u| u.field.is_none()));
    assert_eq!(parse_fields(&content).len(), 2, "fields entries untouched");
}

// ---------------------------------------------------------------------------
// remove-field: a sub-field is removed from an inline multi-field entry; the
// entry itself and its other sub-fields survive.
// ---------------------------------------------------------------------------

#[test]
fn remove_field_removes_inline_sub_field() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::remove_field("pool", "spawn_fn").expect("remove_field should succeed");

    let content = read_back(&dir, "pool");
    let uses = parse_uses(&content);
    assert_eq!(uses.len(), 3, "no whole entry should be removed");
    let spawn = &uses[1];
    assert_eq!(spawn.feature.as_deref(), Some("dynamic"));
    let names: Vec<&str> = spawn.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["spawn_ref", "pending_task_id"]);
}

// ---------------------------------------------------------------------------
// remove-field: a sub-field is removed from a nested [[context.uses.fields]]
// entry as well.
// ---------------------------------------------------------------------------

#[test]
fn remove_field_removes_nested_sub_field() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    context_cmd::remove_field("pool", "timer_ref").expect("remove_field should succeed");

    let content = read_back(&dir, "pool");
    let uses = parse_uses(&content);
    assert_eq!(uses.len(), 3);
    let names: Vec<&str> = uses[2].fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["current_timer"]);
}

// ---------------------------------------------------------------------------
// remove-field: an entry whose last sub-fields are removed is dropped
// entirely (no empty [[context.uses]] husk left behind) — for both the
// inline and nested representations.
// ---------------------------------------------------------------------------

#[test]
fn remove_field_drops_use_entry_left_without_sub_fields() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    for name in ["spawn_fn", "spawn_ref", "pending_task_id"] {
        context_cmd::remove_field("pool", name).expect("inline sub-field removal");
    }
    for name in ["timer_ref", "current_timer"] {
        context_cmd::remove_field("pool", name).expect("nested sub-field removal");
    }

    let content = read_back(&dir, "pool");
    let uses = parse_uses(&content);
    assert_eq!(
        uses.len(),
        1,
        "both emptied multi-field entries must be dropped:\n{content}"
    );
    assert_eq!(uses[0].field.as_deref(), Some("self_ref"));
    for gone in [
        "spawn_fn",
        "spawn_ref",
        "pending_task_id",
        "timer_ref",
        "current_timer",
    ] {
        assert!(
            !content.contains(gone),
            "'{gone}' must not remain:\n{content}"
        );
    }
    assert_eq!(parse_fields(&content).len(), 2, "fields entries untouched");
}

// ---------------------------------------------------------------------------
// remove-field: no matching entry or sub-field → loud not-found (exit 3).
// ---------------------------------------------------------------------------

#[test]
fn remove_field_missing_name_is_not_found_coded() {
    let dir = write_fixture("pool", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    let err = context_cmd::remove_field("pool", "nope").expect_err("missing field should fail");
    assert_eq!(
        exit::code_of(&err),
        Some(exit::EXIT_NOT_FOUND),
        "remove-field of a missing name must map to exit code 3"
    );

    // The failed remove must not have modified the file.
    let content = read_back(&dir, "pool");
    assert_eq!(parse_uses(&content).len(), 3);
    assert_eq!(parse_fields(&content).len(), 2);
}
