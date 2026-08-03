// Copyright 2025 Bloxide, all rights reserved
//! Library-level integration tests for the `cargo-blox add-message` /
//! `remove-message` commands.
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

#[path = "../src/exit.rs"]
mod exit;
#[path = "../src/message_cmd.rs"]
mod message_cmd;
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

/// The blox.toml fixture: copyright header, section comments, and two
/// variants (Ping with a field, Resume without).
const FIXTURE: &str = "\
# Copyright 2025 Bloxide, all rights reserved
# Hand-maintained manifest — comments must survive edits.

[[messages]]
name = \"TestMsg\"
visibility = \"pub\"
copy = true

# Ping variant — sent by the peer each round.
[[messages.variants]]
name = \"Ping\"

  [[messages.variants.fields]]
  name = \"round\"
  ty = \"u32\"

# Resume variant — unit-like.
[[messages.variants]]
name = \"Resume\"
";

/// Writes the fixture to `<temp>/crates/messages/<crate_name>/blox.toml` plus
/// a workspace-root Cargo.toml, and returns the temp dir.
fn write_fixture(crate_name: &str, content: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("write workspace Cargo.toml");
    let msg_dir = dir.path().join("crates/messages").join(crate_name);
    fs::create_dir_all(&msg_dir).expect("create messages dir");
    fs::write(msg_dir.join("blox.toml"), content).expect("write blox.toml");
    dir
}

/// Reads back the fixture blox.toml as a raw string.
fn read_back(dir: &TempDir, crate_name: &str) -> String {
    let path = dir
        .path()
        .join("crates/messages")
        .join(crate_name)
        .join("blox.toml");
    fs::read_to_string(&path).expect("read blox.toml back")
}

/// Variant names of the first `[[messages]]` table, in order.
fn variant_names(content: &str) -> Vec<String> {
    let doc: toml::Value = toml::from_str(content).expect("parse blox.toml back");
    doc.get("messages")
        .and_then(|m| m.as_array())
        .and_then(|a| a.first())
        .and_then(|t| t.get("variants"))
        .and_then(|v| v.as_array())
        .expect("variants array exists")
        .iter()
        .map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .expect("variant name")
                .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Regression: add-message must preserve the copyright header and comments
// (it previously reserialized the document, stripping both).
// ---------------------------------------------------------------------------

#[test]
fn add_message_preserves_header_and_comments() {
    let dir = write_fixture("test-messages", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    message_cmd::add_message(
        "test-messages",
        "Pong",
        vec![("round".to_string(), "u32".to_string())],
    )
    .expect("add_message should succeed");

    let content = read_back(&dir, "test-messages");
    assert!(
        content.contains("# Copyright 2025 Bloxide, all rights reserved"),
        "copyright header must be preserved:\n{content}"
    );
    assert!(
        content.contains("# Ping variant — sent by the peer each round."),
        "variant comment must be preserved:\n{content}"
    );
    assert!(
        content.contains("# Resume variant — unit-like."),
        "second comment must be preserved:\n{content}"
    );
    assert_eq!(variant_names(&content), vec!["Ping", "Resume", "Pong"]);
    // The new variant carries its field.
    let doc: toml::Value = toml::from_str(&content).expect("parse blox.toml back");
    let pong = doc["messages"][0]["variants"][2]
        .as_table()
        .expect("Pong is a table");
    assert_eq!(pong["fields"][0]["name"].as_str(), Some("round"));
    assert_eq!(pong["fields"][0]["ty"].as_str(), Some("u32"));
}

// ---------------------------------------------------------------------------
// remove-message removes exactly the named variant.
// ---------------------------------------------------------------------------

#[test]
fn remove_message_removes_right_variant() {
    let dir = write_fixture("test-messages", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    message_cmd::remove_message("test-messages", "Resume").expect("remove_message should succeed");

    let content = read_back(&dir, "test-messages");
    assert_eq!(variant_names(&content), vec!["Ping"]);
    assert!(
        content.contains("# Copyright 2025 Bloxide, all rights reserved"),
        "copyright header must be preserved:\n{content}"
    );
    assert!(
        content.contains("# Ping variant — sent by the peer each round."),
        "remaining variant comment must be preserved:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Duplicate add-message fails with conflict (exit-code-5) semantics, via
// error downcasting — not string matching.
// ---------------------------------------------------------------------------

#[test]
fn duplicate_add_message_is_conflict_coded() {
    let dir = write_fixture("test-messages", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    let err = message_cmd::add_message("test-messages", "Ping", vec![])
        .expect_err("duplicate variant should fail");
    assert_eq!(
        exit::code_of(&err),
        Some(exit::EXIT_CONFLICT),
        "duplicate add-message must map to exit code 5"
    );
    let coded = err
        .downcast_ref::<exit::CodedError>()
        .expect("error should be a CodedError");
    assert_eq!(coded.code, exit::EXIT_CONFLICT);

    // The failed add must not have modified the file.
    let content = read_back(&dir, "test-messages");
    assert_eq!(variant_names(&content), vec!["Ping", "Resume"]);
}

// ---------------------------------------------------------------------------
// Removing a variant that does not exist is a not-found (exit-code-3) error.
// ---------------------------------------------------------------------------

#[test]
fn remove_nonexistent_variant_is_not_found_coded() {
    let dir = write_fixture("test-messages", FIXTURE);
    let _cwd = CwdGuard::new(dir.path());

    let err = message_cmd::remove_message("test-messages", "Nope")
        .expect_err("removing a missing variant should fail");
    assert_eq!(
        exit::code_of(&err),
        Some(exit::EXIT_NOT_FOUND),
        "remove-message of a missing variant must map to exit code 3"
    );
}

// ---------------------------------------------------------------------------
// add-message targets the first [[messages]] table when the crate has no
// conventional XxxMsg table.
// ---------------------------------------------------------------------------

#[test]
fn add_message_falls_back_to_first_messages_table() {
    const CUSTOM: &str = "\
# Copyright 2025 Bloxide, all rights reserved

[[messages]]
name = \"CustomProto\"

[[messages.variants]]
name = \"Init\"
";
    let dir = write_fixture("custom", CUSTOM);
    let _cwd = CwdGuard::new(dir.path());

    message_cmd::add_message("custom", "Shutdown", vec![]).expect("add_message should succeed");

    let content = read_back(&dir, "custom");
    // No new table created: the variant lands in CustomProto.
    let doc: toml::Value = toml::from_str(&content).expect("parse blox.toml back");
    let tables = doc["messages"].as_array().expect("messages is array");
    assert_eq!(tables.len(), 1, "no new messages table should be created");
    assert_eq!(variant_names(&content), vec!["Init", "Shutdown"]);
}
