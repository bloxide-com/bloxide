// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the spec lint checks in `cargo blox lint`:
//! template conformance (missing sections, stale `Guard::` vocabulary) and
//! dead doc references.
//!
//! The extraction helpers are unit-tested in-process (the module is
//! included via `#[path]` since cargo-blox is a binary crate); the lint
//! pass itself is tested end-to-end by spawning the binary on fixture
//! workspaces.

// The #[path]-included modules bring in more than these tests use.
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

#[path = "../src/lint.rs"]
mod lint;
#[path = "../src/toml_helpers.rs"]
mod toml_helpers;
#[path = "../src/utils.rs"]
mod utils;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

// ---------------------------------------------------------------------------
// Unit level: inline-code path extraction.
// ---------------------------------------------------------------------------

#[test]
fn extract_finds_inline_workspace_paths() {
    let md = "The blox lives in `crates/bloxes/ping/` and the app in `apps/tokio-demo`.";
    let refs = lint::extract_doc_refs(md);
    let spans: Vec<&str> = refs.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(spans, vec!["crates/bloxes/ping/", "apps/tokio-demo"]);
    assert_eq!(refs[0].0, 1, "1-based line number");
}

#[test]
fn extract_skips_fenced_code_blocks() {
    let md =
        "Before\n```rust\n// crates/fake/in-fence/\n```\nAfter `runtimes/bloxide-tokio/` here.";
    let refs = lint::extract_doc_refs(md);
    let spans: Vec<&str> = refs.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(spans, vec!["runtimes/bloxide-tokio/"]);
}

#[test]
fn extract_skips_placeholders_and_globs() {
    let md = "Paths: `crates/bloxes/<name>/`, `runtimes/*/src/`, `crates/bloxes/{a,b}/`, `crates/real/`.";
    let refs = lint::extract_doc_refs(md);
    let spans: Vec<&str> = refs.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(spans, vec!["crates/real/"]);
}

#[test]
fn extract_ignores_non_workspace_spans() {
    let md = "Use `Decision::Stop` and `spec/bloxes/` and `src/generated/` here.";
    assert!(lint::extract_doc_refs(md).is_empty());
}

#[test]
fn normalize_strips_suffixes() {
    assert_eq!(
        lint::normalize_doc_path("crates/tools/bloxide-codegen/src/wiring.rs::validate"),
        Some("crates/tools/bloxide-codegen/src/wiring.rs".to_string())
    );
    assert_eq!(
        lint::normalize_doc_path("crates/bloxes/ping/blox.toml:42"),
        Some("crates/bloxes/ping/blox.toml".to_string())
    );
    assert_eq!(
        lint::normalize_doc_path("apps/tokio-demo#main"),
        Some("apps/tokio-demo".to_string())
    );
    assert_eq!(
        lint::normalize_doc_path("crates/blox-ctx-ping-pong/"),
        Some("crates/blox-ctx-ping-pong".to_string())
    );
    assert_eq!(lint::normalize_doc_path("crates/"), None);
    assert_eq!(lint::normalize_doc_path("tools/"), None);
}

// ---------------------------------------------------------------------------
// End to end: the lint pass on fixture workspaces.
// ---------------------------------------------------------------------------

/// Minimal valid blox.toml for a blox crate (no topology → no topology
/// diagnostics).
const BLOX_FIXTURE: &str = "\
[actor]
name = \"Foo\"
";

/// Writes a lint fixture workspace: `[workspace]` Cargo.toml +
/// `crates/bloxes/foo/blox.toml`. Returns the temp dir.
fn write_lint_fixture() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("write workspace Cargo.toml");
    let blox_dir = dir.path().join("crates/bloxes/foo");
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), BLOX_FIXTURE).expect("write blox.toml");
    dir
}

fn write_spec(dir: &TempDir, rel: &str, content: &str) {
    let path = dir.path().join(rel);
    fs::create_dir_all(path.parent().expect("spec parent")).expect("create spec dir");
    fs::write(path, content).expect("write spec");
}

/// Runs `cargo-blox blox lint` in `dir` and returns (stdout, stderr, success).
fn run_lint(dir: &TempDir) -> (String, String, bool) {
    let output = Command::new(blox_bin())
        .current_dir(dir.path())
        .arg("blox")
        .arg("lint")
        .output()
        .expect("spawn cargo-blox");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

#[test]
fn warns_on_spec_missing_sections_and_stale_vocabulary() {
    let dir = write_lint_fixture();
    write_spec(
        &dir,
        "spec/bloxes/foo.md",
        "# Blox Spec: `Foo`\n\n## Purpose\n\nDoes things. Self-suspends via `Guard::Stop`.\n",
    );

    let (stdout, _stderr, success) = run_lint(&dir);
    assert!(success, "warnings must not fail the lint run");
    assert!(
        stdout.contains("spec for blox 'foo' lacks the '## blox.toml' section"),
        "missing blox.toml section should warn: {stdout}"
    );
    assert!(
        stdout.contains("spec for blox 'foo' lacks the '## Open Questions' section"),
        "missing Open Questions section should warn: {stdout}"
    );
    assert!(
        stdout.contains("stale `Guard::` vocabulary"),
        "Guard:: usage should warn: {stdout}"
    );
}

#[test]
fn warns_on_dead_doc_references_only() {
    let dir = write_lint_fixture();
    write_spec(
        &dir,
        "spec/architecture/notes.md",
        "# Notes\n\nSee `crates/blox-ctx-ping-pong/` for the context crate. \
The blox lives in `crates/bloxes/foo/`.\n\n```rust\n// crates/fake/in-fence/ must not be flagged\n```\n\
Placeholder `crates/bloxes/<name>/` and glob `runtimes/*/src/` must not be flagged.\n",
    );

    let (stdout, _stderr, success) = run_lint(&dir);
    assert!(success, "warnings must not fail the lint run");
    assert!(
        stdout.contains("`crates/blox-ctx-ping-pong/` which does not exist"),
        "dead reference should warn: {stdout}"
    );
    assert!(
        !stdout.contains("in-fence"),
        "paths inside fenced code blocks must not warn: {stdout}"
    );
    assert!(
        !stdout.contains("<name>") && !stdout.contains("runtimes/*"),
        "placeholders and globs must not warn: {stdout}"
    );
    assert!(
        !stdout.contains("`crates/bloxes/foo/` which does not exist"),
        "existing paths must not warn: {stdout}"
    );
}

#[test]
fn clean_workspace_has_no_warnings() {
    let dir = write_lint_fixture();
    write_spec(
        &dir,
        "spec/bloxes/foo.md",
        "# Blox Spec: `Foo`\n\n## Purpose\n\nDoes things. Self-suspends via `Decision::Stop`.\n\n## blox.toml\n\nDrives codegen.\n\n## Open Questions\n\n- [ ] None yet.\n",
    );
    write_spec(
        &dir,
        "spec/architecture/notes.md",
        "# Notes\n\nThe blox lives in `crates/bloxes/foo/`.\n",
    );

    let (stdout, _stderr, success) = run_lint(&dir);
    assert!(success, "lint should succeed");
    assert!(
        stdout.contains("0 error(s), 0 warning(s)"),
        "clean fixture should have no warnings: {stdout}"
    );
}

#[test]
fn blox_without_spec_is_not_checked() {
    let dir = write_lint_fixture();
    // No spec/bloxes/foo.md at all — conformance check must skip it.
    let (stdout, _stderr, success) = run_lint(&dir);
    assert!(success, "lint should succeed");
    assert!(
        stdout.contains("0 error(s), 0 warning(s)"),
        "a blox without a spec should produce no warnings: {stdout}"
    );
}
