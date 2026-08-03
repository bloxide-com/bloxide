// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for schema and use-site validation in bloxide-codegen,
//! driven through its public API (`generate_from_toml`,
//! `generate_system_wiring_from_toml`) with fixture TOMLs.
//!
//! Covered:
//! - `[[context.uses]] role` must be `ctor` or `state` (a typo fails
//!   generation with a clear error)
//! - unknown TOML keys are rejected (`deny_unknown_fields`)
//! - an action with `kind = "entry"` wired in a transition slot is a hard
//!   error at system codegen
//! - unknown supervision strategies are a hard error at system codegen

use std::fs;

use tempfile::TempDir;

/// Writes a blox.toml to `<temp>/crates/bloxes/<name>/blox.toml` (the parent
/// dir name doubles as the crate name for codegen) and returns the temp dir.
fn write_blox(dir: &TempDir, name: &str, content: &str) {
    let blox_dir = dir.path().join("crates/bloxes").join(name);
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), content).expect("write blox.toml");
}

fn blox_toml_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path()
        .join("crates/bloxes")
        .join(name)
        .join("blox.toml")
}

fn write_system(dir: &TempDir, app: &str, content: &str) -> std::path::PathBuf {
    let app_dir = dir.path().join("apps").join(app);
    fs::create_dir_all(&app_dir).expect("create app dir");
    let path = app_dir.join("system.toml");
    fs::write(&path, content).expect("write system.toml");
    path
}

// ---------------------------------------------------------------------------
// role validation: "ctro" (typo) fails generation; "ctor" is accepted.
// ---------------------------------------------------------------------------

const ROLE_TYPO_FIXTURE: &str = "\
[context]
name = \"RoleCtx\"
generics = \"<R: BloxRuntime>\"

[[context.uses]]
crate = \"some_crate\"
field = \"peer_ref\"
field_type = \"u32\"
role = \"ctro\"
";

#[test]
fn invalid_role_fails_generation() {
    let dir = TempDir::new().expect("create temp dir");
    write_blox(&dir, "roletest", ROLE_TYPO_FIXTURE);

    let err = bloxide_codegen::generate_from_toml(&blox_toml_path(&dir, "roletest"))
        .expect_err("role typo should fail generation");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("invalid role 'ctro'"),
        "error should name the invalid role: {msg}"
    );
    assert!(
        msg.contains("ctor") && msg.contains("state"),
        "error should name the valid roles: {msg}"
    );
}

#[test]
fn valid_role_accepted() {
    let dir = TempDir::new().expect("create temp dir");
    write_blox(&dir, "roletest", &ROLE_TYPO_FIXTURE.replace("ctro", "ctor"));

    bloxide_codegen::generate_from_toml(&blox_toml_path(&dir, "roletest"))
        .expect("role = \"ctor\" should generate cleanly");
}

// ---------------------------------------------------------------------------
// Schema validation: unknown TOML keys are rejected (deny_unknown_fields).
// ---------------------------------------------------------------------------

#[test]
fn unknown_toml_key_fails() {
    const FIXTURE: &str = "\
bogus_key = true

[actor]
name = \"Test\"
";
    let dir = TempDir::new().expect("create temp dir");
    write_blox(&dir, "unktest", FIXTURE);

    let err = bloxide_codegen::generate_from_toml(&blox_toml_path(&dir, "unktest"))
        .expect_err("unknown key should fail parsing");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown field") && msg.contains("bogus_key"),
        "error should report the unknown field: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Use-site validation at system codegen: kind = "entry" wired in a
// transition slot is a hard error.
// ---------------------------------------------------------------------------

/// A blox with one action `do_thing`; `{{KIND}}` is substituted per test.
const BLOX_KIND_FIXTURE: &str = "\
[actor]
name = \"Test\"

[context]
name = \"TestCtx\"

[[context.actions]]
name = \"do_thing\"
crate = \"some_crate\"
kind = \"{{KIND}}\"

[event]
name = \"TestEvent\"

[[event.mailboxes]]
variant = \"Msg\"
message = \"TestMsg\"
message_path = \"test_messages::TestMsg\"

[topology]

[[topology.states]]
name = \"Ready\"
initial = true

[[topology.transitions]]
state = \"Ready\"
event = \"TestMsg::Tick(_)\"
target = \"stay\"
actions = [\"Self::do_thing\"]
";

const SYSTEM_FIXTURE: &str = "\
[system]
runtime = \"tokio\"

[[actors]]
name = \"test\"
blox = \"test-blox\"

[[supervision]]
supervisor = \"bloxide-supervisor\"
strategy = \"when_any_done\"
children = [\"test\"]
";

#[test]
fn entry_action_in_transition_slot_fails_system_codegen() {
    let dir = TempDir::new().expect("create temp dir");
    write_blox(
        &dir,
        "test-blox",
        &BLOX_KIND_FIXTURE.replace("{{KIND}}", "entry"),
    );
    let system_path = write_system(&dir, "test-app", SYSTEM_FIXTURE);

    let err = bloxide_codegen::generate_system_wiring_from_toml(&system_path, dir.path())
        .expect_err("kind mismatch should fail system codegen");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("do_thing") && msg.contains("kind = \"entry\""),
        "error should name the action and its kind: {msg}"
    );
    assert!(
        msg.contains("transition slot"),
        "error should name the use site: {msg}"
    );
}

/// Control: the same fixture with the correct kind generates cleanly,
/// proving the failure above is the kind validation and not the fixture.
#[test]
fn transition_action_in_transition_slot_accepted() {
    let dir = TempDir::new().expect("create temp dir");
    write_blox(
        &dir,
        "test-blox",
        &BLOX_KIND_FIXTURE.replace("{{KIND}}", "transition"),
    );
    let system_path = write_system(&dir, "test-app", SYSTEM_FIXTURE);

    bloxide_codegen::generate_system_wiring_from_toml(&system_path, dir.path())
        .expect("kind = \"transition\" should generate cleanly");
}

// ---------------------------------------------------------------------------
// Unknown supervision strategies are a hard error at system codegen (the
// wiring generator, independent of the CLI's add-supervision check).
// ---------------------------------------------------------------------------

#[test]
fn unknown_strategy_fails_system_codegen() {
    /// A blox without actions, so nothing else can fail first.
    const BLOX_NO_ACTIONS: &str = "\
[actor]
name = \"Test\"

[context]
name = \"TestCtx\"

[event]
name = \"TestEvent\"

[[event.mailboxes]]
variant = \"Msg\"
message = \"TestMsg\"
message_path = \"test_messages::TestMsg\"

[topology]

[[topology.states]]
name = \"Ready\"
initial = true
";
    const BAD_STRATEGY_SYSTEM: &str = "\
[system]
runtime = \"tokio\"

[[actors]]
name = \"test\"
blox = \"test-blox\"

[[supervision]]
supervisor = \"bloxide-supervisor\"
strategy = \"one_for_one\"
children = [\"test\"]
";
    let dir = TempDir::new().expect("create temp dir");
    write_blox(&dir, "test-blox", BLOX_NO_ACTIONS);
    let system_path = write_system(&dir, "test-app", BAD_STRATEGY_SYSTEM);

    let err = bloxide_codegen::generate_system_wiring_from_toml(&system_path, dir.path())
        .expect_err("unknown strategy should fail system codegen");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown supervision strategy 'one_for_one'"),
        "error should name the bad strategy: {msg}"
    );
}
