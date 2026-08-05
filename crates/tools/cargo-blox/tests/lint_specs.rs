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
    write_lint_fixture_with(BLOX_FIXTURE)
}

/// Same as `write_lint_fixture`, with caller-supplied blox.toml content.
fn write_lint_fixture_with(blox_toml: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("write workspace Cargo.toml");
    let blox_dir = dir.path().join("crates/bloxes/foo");
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), blox_toml).expect("write blox.toml");
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

// ---------------------------------------------------------------------------
// End to end: context-section checks (duplicates, returns contract).
// ---------------------------------------------------------------------------

#[test]
fn errors_on_duplicate_action_names() {
    let dir = write_lint_fixture_with(
        "\
[actor]
name = \"Foo\"

[context]
name = \"FooCtx\"

[[context.actions]]
name = \"do_thing\"
crate = \"some_crate\"

[[context.actions]]
name = \"do_other\"
crate = \"some_crate\"

[[context.actions]]
name = \"do_thing\"
crate = \"some_crate\"
",
    );

    let (_stdout, stderr, success) = run_lint(&dir);
    assert!(!success, "duplicate action names must fail the lint run");
    assert!(
        stderr.contains("duplicate action name \"do_thing\""),
        "duplicate action name should error: {stderr}"
    );
    assert!(
        !stderr.contains("duplicate action name \"do_other\""),
        "unique action names must not error: {stderr}"
    );
}

#[test]
fn errors_on_invalid_action_returns() {
    let dir = write_lint_fixture_with(
        "\
[actor]
name = \"Foo\"

[context]
name = \"FooCtx\"

[[context.actions]]
name = \"good\"
crate = \"some_crate\"
returns = \"ActionResult\"

[[context.actions]]
name = \"bad\"
crate = \"some_crate\"
returns = \"bool\"
",
    );

    let (_stdout, stderr, success) = run_lint(&dir);
    assert!(
        !success,
        "an unrecognized returns value must fail the lint run"
    );
    assert!(
        stderr.contains(
            "action \"bad\" has returns = \"bool\" — the only recognized value is \"ActionResult\""
        ),
        "invalid returns should error with the codegen message: {stderr}"
    );
    assert!(
        !stderr.contains("action \"good\" has returns"),
        "returns = \"ActionResult\" must not error: {stderr}"
    );
}

#[test]
fn errors_on_duplicate_context_field_names() {
    let dir = write_lint_fixture_with(
        "\
[actor]
name = \"Foo\"

[context]
name = \"FooCtx\"

[[context.fields]]
name = \"count\"
type = \"u32\"

[[context.fields]]
name = \"total\"
type = \"u32\"

[[context.fields]]
name = \"count\"
type = \"u32\"
",
    );

    let (_stdout, stderr, success) = run_lint(&dir);
    assert!(
        !success,
        "duplicate context field names must fail the lint run"
    );
    assert!(
        stderr.contains("duplicate context field name \"count\""),
        "duplicate context field name should error: {stderr}"
    );
    assert!(
        !stderr.contains("duplicate context field name \"total\""),
        "unique context field names must not error: {stderr}"
    );
}

#[test]
fn errors_on_duplicate_context_uses_field_names() {
    // Both shapes: single-field entries (`field = "..."`) and multi-field
    // entries (`[[context.uses.fields]]` sub-entries) contribute context
    // struct fields — duplicates collide in the generated struct.
    let dir = write_lint_fixture_with(
        "\
[actor]
name = \"Foo\"

[context]
name = \"FooCtx\"

[[context.uses]]
field = \"peer_ref\"
field_type = \"ActorRef<Msg, R>\"
role = \"ctor\"

[[context.uses]]
field = \"peer_ref\"
field_type = \"ActorRef<Msg, R>\"
role = \"ctor\"

[[context.uses]]
fields = [
    { name = \"spawn_fn\", ty = \"u32\", role = \"ctor\" },
    { name = \"spawn_fn\", ty = \"u32\", role = \"state\" },
]
",
    );

    let (_stdout, stderr, success) = run_lint(&dir);
    assert!(
        !success,
        "duplicate context uses field names must fail the lint run"
    );
    assert!(
        stderr.contains("duplicate context uses field name \"peer_ref\""),
        "duplicate single-field uses name should error: {stderr}"
    );
    assert!(
        stderr.contains("duplicate context uses field name \"spawn_fn\""),
        "duplicate multi-field uses name should error: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// End to end: action references against spec_imports.
// ---------------------------------------------------------------------------

/// Regression test: the old substring match (`imp.contains(last)`) accepted
/// `stop_all` because it is a substring of the imported `stop_all_children` —
/// a false negative. Exact leaf-segment matching rejects it.
#[test]
fn errors_on_action_ref_sharing_prefix_with_import() {
    let dir = write_lint_fixture_with(
        "\
[actor]
name = \"Foo\"

[topology]
spec_imports = [\"some_crate::stop_all_children\"]

[[topology.states]]
name = \"Ready\"
initial = true

[[topology.transitions]]
state = \"Ready\"
event = \"FooMsg::Bar(_)\"
target = \"stay\"
actions = [\"stop_all\"]
",
    );

    let (_stdout, stderr, success) = run_lint(&dir);
    assert!(!success, "an unimported action must fail the lint run");
    assert!(
        stderr.contains("action `stop_all` does not appear in any spec_imports entry"),
        "prefix-sharing action ref should error: {stderr}"
    );
}

#[test]
fn accepts_action_refs_matching_import_leaves() {
    let dir = write_lint_fixture_with(
        "\
[actor]
name = \"Foo\"

[topology]
spec_imports = [
    \"some_crate::stop_all_children\",
    \"other_crate::{send_ping, send_pong}\",
]

[[topology.states]]
name = \"Ready\"
initial = true

[[topology.transitions]]
state = \"Ready\"
event = \"FooMsg::Bar(_)\"
target = \"stay\"
actions = [\"stop_all_children\", \"send_ping\", \"other_crate::send_pong\"]
",
    );

    let (stdout, _stderr, success) = run_lint(&dir);
    assert!(success, "exact leaf matches must pass the lint run");
    assert!(
        stdout.contains("0 error(s), 0 warning(s)"),
        "imported action refs should produce no diagnostics: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// End to end: duplicate-transition checks are feature-gate aware.
// ---------------------------------------------------------------------------

/// Two transitions may share a `(state, event)` pair when their `feature`
/// gates differ — they are distinct variants, not duplicates.
#[test]
fn feature_gated_transition_variants_are_not_duplicates() {
    let dir = write_lint_fixture_with(
        "\
[actor]
name = \"Foo\"

[topology]

[[topology.states]]
name = \"Ready\"
initial = true

[[topology.states]]
name = \"Active\"

[[topology.transitions]]
state = \"Ready\"
event = \"FooMsg::Bar(_)\"
target = \"Active\"

[[topology.transitions]]
state = \"Ready\"
event = \"FooMsg::Bar(_)\"
target = \"stay\"
feature = \"dynamic\"

[[topology.transitions]]
state = \"Active\"
event = \"FooMsg::Baz(_)\"
target = \"stay\"
",
    );

    let (stdout, stderr, success) = run_lint(&dir);
    assert!(
        success,
        "feature-gated variants must pass the lint run: {stderr}"
    );
    assert!(
        !stderr.contains("duplicate transition"),
        "feature-differing duplicates must not be flagged: {stderr}"
    );
    assert!(
        stdout.contains("0 error(s), 0 warning(s)"),
        "no diagnostics expected: {stdout}"
    );
}

/// Positive control: an exact duplicate — same state, event, AND feature —
/// is still flagged.
#[test]
fn errors_on_duplicate_transition_with_same_feature() {
    let dir = write_lint_fixture_with(
        "\
[actor]
name = \"Foo\"

[topology]

[[topology.states]]
name = \"Ready\"
initial = true

[[topology.states]]
name = \"Active\"

[[topology.transitions]]
state = \"Ready\"
event = \"FooMsg::Bar(_)\"
target = \"Active\"
feature = \"dynamic\"

[[topology.transitions]]
state = \"Ready\"
event = \"FooMsg::Bar(_)\"
target = \"stay\"
feature = \"dynamic\"
",
    );

    let (_stdout, stderr, success) = run_lint(&dir);
    assert!(!success, "exact duplicates must fail the lint run");
    assert!(
        stderr.contains(
            "duplicate transition in state \"Ready\" for event pattern `FooMsg::Bar(_)` (feature \"dynamic\")"
        ),
        "same-feature duplicates should error: {stderr}"
    );
}
