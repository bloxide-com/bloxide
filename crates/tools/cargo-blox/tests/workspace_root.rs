// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for workspace-root resolution: subcommands must work
//! when run from any subdirectory of the workspace, not just the root.
//!
//! Each test builds a fixture workspace (with a `[workspace]` Cargo.toml at
//! the root), spawns the `cargo-blox` binary with its CWD set to a
//! SUBDIRECTORY, and asserts the command still finds and edits the right
//! file under the root.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

const MESSAGES_FIXTURE: &str = "\
[[messages]]
name = \"TestMsg\"

[[messages.variants]]
name = \"Ping\"
";

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
///   Cargo.toml ([workspace])
///   crates/messages/test-messages/blox.toml
///   bloxes/wsblox/blox.toml
fn write_fixture() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("write workspace Cargo.toml");
    let msg_dir = dir.path().join("crates/messages/test-messages");
    fs::create_dir_all(&msg_dir).expect("create messages dir");
    fs::write(msg_dir.join("blox.toml"), MESSAGES_FIXTURE).expect("write messages blox.toml");
    let blox_dir = dir.path().join("bloxes/wsblox");
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), BLOX_FIXTURE).expect("write blox blox.toml");
    dir
}

/// Runs `cargo-blox blox <args...>` with `cwd` set to `cwd` and returns
/// (stdout, stderr, success).
fn run_blox_in(cwd: &PathBuf, args: &[&str]) -> (String, String, bool) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(cwd);
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
fn list_messages_from_crate_subdirectory() {
    let dir = write_fixture();
    let cwd = dir.path().join("crates/messages/test-messages");
    let (stdout, stderr, success) = run_blox_in(&cwd, &["list-messages", "test-messages"]);
    assert!(
        success,
        "list-messages from a subdirectory should succeed: {stderr}"
    );
    assert!(
        stdout.contains("Ping"),
        "variant Ping should be listed: {stdout}"
    );
}

#[test]
fn list_states_from_crate_subdirectory() {
    let dir = write_fixture();
    let cwd = dir.path().join("bloxes/wsblox");
    let (stdout, stderr, success) = run_blox_in(&cwd, &["list-states", "wsblox"]);
    assert!(
        success,
        "list-states from a subdirectory should succeed: {stderr}"
    );
    assert!(
        stdout.contains("Idle"),
        "state Idle should be listed: {stdout}"
    );
    assert!(
        stdout.contains("Active"),
        "state Active should be listed: {stdout}"
    );
}

#[test]
fn add_message_from_nested_subdirectory_edits_workspace_file() {
    let dir = write_fixture();
    // A nested subdirectory that is NOT the crate dir.
    let cwd = dir.path().join("crates/messages");
    let (_stdout, stderr, success) = run_blox_in(&cwd, &["add-message", "test-messages", "Pong"]);
    assert!(
        success,
        "add-message from a subdirectory should succeed: {stderr}"
    );

    // The file under the workspace ROOT (not ./crates/messages relative to
    // the subdir) must have been edited.
    let content = fs::read_to_string(dir.path().join("crates/messages/test-messages/blox.toml"))
        .expect("read blox.toml back");
    assert!(
        content.contains("Pong"),
        "Pong should be added to the workspace-root file:\n{content}"
    );
}

// ── new-* scaffolding commands ──────────────────────────────────────────────
//
// The `new-*` commands must also anchor on the workspace root (walk up from
// the invocation directory, like watch/wire) instead of writing `crates/…`,
// `examples/…`, and `Cargo.toml` relative to the current directory.

const IMPL_BLOX_FIXTURE: &str = "\
[actor]
name = \"Demo\"

[context]
name = \"DemoCtx\"

[topology]
[[topology.states]]
name = \"Idle\"
initial = true
";

/// Writes a bare fixture workspace for the scaffolding commands:
///   Cargo.toml ([workspace] members + [workspace.dependencies])
///   crates/               (the subdirectory the commands run from)
fn write_scaffold_fixture() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    // bloxide-core is referenced by every scaffolded blox's derived
    // dependencies — the materializer resolves it via [workspace.dependencies].
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\n]\n\n[workspace.dependencies]\nbloxide-core = { path = \"crates/bloxide-core\" }\n",
    )
    .expect("write workspace Cargo.toml");
    fs::create_dir_all(dir.path().join("crates")).expect("create crates dir");
    dir
}

/// Asserts the root Cargo.toml picked up the scaffolded crate and that
/// nothing was created relative to the invocation subdirectory.
fn assert_registered(cwd: &PathBuf, member: &str, root_toml: &str) {
    assert!(
        root_toml.contains(&format!("\"{member}\"")),
        "member '{member}' should be registered in the root Cargo.toml:\n{root_toml}"
    );
    for polluted in ["crates", "examples", "spec", "Cargo.toml"] {
        assert!(
            !cwd.join(polluted).exists(),
            "nothing should be scaffolded relative to the invocation subdirectory: {}",
            cwd.join(polluted).display()
        );
    }
}

#[test]
fn new_messages_from_subdirectory_scaffolds_at_root() {
    let dir = write_scaffold_fixture();
    let cwd = dir.path().join("crates");
    let (_stdout, stderr, success) = run_blox_in(&cwd, &["new-messages", "demo"]);
    assert!(
        success,
        "new-messages from a subdirectory should succeed: {stderr}"
    );

    assert!(dir
        .path()
        .join("crates/messages/demo-messages/blox.toml")
        .exists());
    assert!(dir
        .path()
        .join("crates/messages/demo-messages/Cargo.toml")
        .exists());
    assert!(dir
        .path()
        .join("crates/messages/demo-messages/src/lib.rs")
        .exists());
    let root_toml =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");
    assert!(
        root_toml.contains("demo-messages = { path = \"crates/messages/demo-messages\" }"),
        "workspace dependency should be registered:\n{root_toml}"
    );
    assert_registered(&cwd, "crates/messages/demo-messages", &root_toml);
}

#[test]
fn new_messages_from_root_scaffolds_at_root() {
    let dir = write_scaffold_fixture();
    let cwd = dir.path().to_path_buf();
    let (_stdout, stderr, success) = run_blox_in(&cwd, &["new-messages", "demo"]);
    assert!(
        success,
        "new-messages from the root should succeed: {stderr}"
    );
    assert!(dir
        .path()
        .join("crates/messages/demo-messages/blox.toml")
        .exists());
}

#[test]
fn new_context_from_subdirectory_scaffolds_at_root() {
    let dir = write_scaffold_fixture();
    let cwd = dir.path().join("crates");
    let (_stdout, stderr, success) = run_blox_in(&cwd, &["new-context", "demo"]);
    assert!(
        success,
        "new-context from a subdirectory should succeed: {stderr}"
    );

    assert!(dir
        .path()
        .join("crates/context/blox-ctx-demo/Cargo.toml")
        .exists());
    assert!(dir
        .path()
        .join("crates/context/blox-ctx-demo/src/lib.rs")
        .exists());
    let root_toml =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");
    assert!(
        root_toml.contains("blox-ctx-demo = { path = \"crates/context/blox-ctx-demo\" }"),
        "workspace dependency should be registered:\n{root_toml}"
    );
    assert_registered(&cwd, "crates/context/blox-ctx-demo", &root_toml);
}

#[test]
fn new_blox_from_subdirectory_scaffolds_at_root() {
    let dir = write_scaffold_fixture();
    let cwd = dir.path().join("crates");
    let (_stdout, stderr, success) = run_blox_in(&cwd, &["new", "demo"]);
    assert!(success, "new from a subdirectory should succeed: {stderr}");

    // Pure-TOML layout: bloxes/<name>/blox.toml only — no Cargo.toml, no
    // src/, no workspace registration (the crate is materialized into
    // target/bloxide-generated by `cargo blox generate`).
    assert!(dir.path().join("bloxes/demo/blox.toml").exists());
    assert!(!dir.path().join("crates/bloxes/demo").exists());
    assert!(dir.path().join("spec/bloxes/demo.md").exists());
    let root_toml =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");
    assert!(
        !root_toml.contains("demo-blox"),
        "pure-TOML bloxes are not registered in the root Cargo.toml:\n{root_toml}"
    );
    for polluted in ["crates", "bloxes", "examples", "spec", "Cargo.toml"] {
        assert!(
            !cwd.join(polluted).exists(),
            "nothing should be scaffolded relative to the invocation subdirectory: {}",
            cwd.join(polluted).display()
        );
    }
}

#[test]
fn new_impl_from_subdirectory_scaffolds_at_root() {
    let dir = write_scaffold_fixture();
    let blox_dir = dir.path().join("bloxes/demo");
    fs::create_dir_all(&blox_dir).expect("create blox dir");
    fs::write(blox_dir.join("blox.toml"), IMPL_BLOX_FIXTURE).expect("write blox.toml");

    let cwd = dir.path().join("crates");
    let (_stdout, stderr, success) = run_blox_in(&cwd, &["new-impl", "demo", "--blox", "demo"]);
    assert!(
        success,
        "new-impl from a subdirectory should succeed: {stderr}"
    );

    assert!(dir.path().join("crates/impl/demo/Cargo.toml").exists());
    assert!(dir.path().join("crates/impl/demo/src/lib.rs").exists());
    let root_toml =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");
    assert!(
        root_toml.contains("demo-impl = { path = \"crates/impl/demo\" }"),
        "workspace dependency should be registered:\n{root_toml}"
    );
    assert_registered(&cwd, "crates/impl/demo", &root_toml);
}

#[test]
fn new_binary_from_subdirectory_scaffolds_at_root() {
    let dir = write_scaffold_fixture();
    let cwd = dir.path().join("crates");
    let (_stdout, stderr, success) = run_blox_in(&cwd, &["new-binary", "demo"]);
    assert!(
        success,
        "new-binary from a subdirectory should succeed: {stderr}"
    );

    assert!(dir.path().join("examples/demo/system.toml").exists());
    // Examples are NOT workspace members (the crate is materialized into
    // target/bloxide-generated/) — the root Cargo.toml stays untouched, and
    // nothing lands relative to the invocation subdirectory.
    let root_toml =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");
    assert!(
        !root_toml.contains("examples/demo"),
        "examples must not be registered as workspace members:\n{root_toml}"
    );
    for polluted in ["examples", "spec", "Cargo.toml"] {
        assert!(
            !cwd.join(polluted).exists(),
            "nothing should be scaffolded relative to the invocation subdirectory: {}",
            cwd.join(polluted).display()
        );
    }
}

#[test]
fn new_all_from_subdirectory_scaffolds_all_layers_at_root() {
    let dir = write_scaffold_fixture();
    let cwd = dir.path().join("crates");
    // Decoy: stands in for the cargo-blox crate dir that CARGO_MANIFEST_DIR
    // points at under `cargo run`. The internal generate pass must anchor on
    // the invocation workspace root, not on CARGO_MANIFEST_DIR.
    let decoy = TempDir::new().expect("create decoy dir");

    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(&cwd);
    cmd.arg("blox");
    cmd.args(["new-all", "demo"]);
    cmd.env("CARGO_MANIFEST_DIR", decoy.path());
    let output = cmd.output().expect("spawn cargo-blox");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "new-all from a subdirectory should succeed: {stderr}"
    );

    // Every layer lands at the workspace root.
    for path in [
        "crates/messages/demo-messages/blox.toml",
        "crates/context/blox-ctx-demo/src/lib.rs",
        "bloxes/demo/blox.toml",
        "crates/impl/demo/src/lib.rs",
        "examples/demo/system.toml",
        "spec/bloxes/demo.md",
    ] {
        assert!(dir.path().join(path).exists(), "missing {path}");
    }
    // The internal generate pass ran against the fixture root (the two
    // scaffolded blox.toml files), not against CARGO_MANIFEST_DIR.
    assert!(
        stdout.contains("processed 2 blox.toml files"),
        "generate should process the fixture workspace blox.toml files:\n{stdout}"
    );
    // The pure-TOML blox materialized into the generated workspace.
    assert!(
        dir.path()
            .join("target/bloxide-generated/crates/demo-blox/Cargo.toml")
            .exists(),
        "demo-blox should be materialized into target/bloxide-generated"
    );
    assert!(
        !decoy.path().join("crates").exists(),
        "nothing should be generated inside the CARGO_MANIFEST_DIR decoy"
    );
    let root_toml =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");
    assert!(
        !root_toml.contains("demo-blox"),
        "pure-TOML bloxes are not registered in the root Cargo.toml:\n{root_toml}"
    );
    for polluted in ["crates", "bloxes", "examples", "spec", "Cargo.toml"] {
        assert!(
            !cwd.join(polluted).exists(),
            "nothing should be scaffolded relative to the invocation subdirectory: {}",
            cwd.join(polluted).display()
        );
    }
}

// ── new-binary runtime validation ───────────────────────────────────────────
//
// `--runtime` accepts only tokio/embassy — the same check `init` runs, shared
// via `utils::validate_runtime`. An invalid value must fail before anything
// is written to the workspace.

#[test]
fn new_binary_with_invalid_runtime_fails_without_writing() {
    let dir = write_scaffold_fixture();
    let cwd = dir.path().to_path_buf();
    let root_toml_before =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");

    let (_stdout, stderr, success) =
        run_blox_in(&cwd, &["new-binary", "demo", "--runtime", "nonsense"]);
    assert!(!success, "new-binary with an invalid runtime should fail");
    assert!(
        stderr.contains("unknown runtime 'nonsense' — expected tokio or embassy"),
        "stderr should name the bad value and the accepted runtimes:\n{stderr}"
    );
    assert!(
        !dir.path().join("examples").exists(),
        "no examples directory should be scaffolded on validation failure"
    );
    let root_toml_after =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");
    assert_eq!(
        root_toml_before, root_toml_after,
        "root Cargo.toml must be untouched on validation failure"
    );
}

#[test]
fn new_binary_with_valid_runtimes_scaffolds() {
    for runtime in ["tokio", "embassy"] {
        let dir = write_scaffold_fixture();
        let cwd = dir.path().to_path_buf();
        let (_stdout, stderr, success) =
            run_blox_in(&cwd, &["new-binary", "demo", "--runtime", runtime]);
        assert!(
            success,
            "new-binary --runtime {runtime} should succeed: {stderr}"
        );
        let system_toml = fs::read_to_string(dir.path().join("examples/demo/system.toml"))
            .expect("read scaffolded system.toml");
        assert!(
            system_toml.contains(&format!("runtime = \"{runtime}\"")),
            "system.toml should record the requested runtime:\n{system_toml}"
        );
    }
}

// ── new-all runtime validation ──────────────────────────────────────────────
//
// `new-all` scaffolds layers 1-4 before the binary layer's own check would
// fire, so it must validate `--runtime` up front — before anything is
// written.

#[test]
fn new_all_with_invalid_runtime_fails_without_writing() {
    let dir = write_scaffold_fixture();
    let cwd = dir.path().to_path_buf();
    let root_toml_before =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");

    let (_stdout, stderr, success) =
        run_blox_in(&cwd, &["new-all", "demo", "--runtime", "nonsense"]);
    assert!(!success, "new-all with an invalid runtime should fail");
    assert!(
        stderr.contains("unknown runtime 'nonsense' — expected tokio or embassy"),
        "stderr should name the bad value and the accepted runtimes:\n{stderr}"
    );
    // The fixture only provides the root Cargo.toml and an empty crates/ dir;
    // no layer may add anything on validation failure.
    let crates_entries = fs::read_dir(dir.path().join("crates"))
        .expect("read crates dir")
        .count();
    assert_eq!(
        crates_entries, 0,
        "no layer crates should be scaffolded on validation failure"
    );
    for polluted in ["examples", "spec"] {
        assert!(
            !dir.path().join(polluted).exists(),
            "no {polluted}/ directory should be scaffolded on validation failure"
        );
    }
    let root_toml_after =
        fs::read_to_string(dir.path().join("Cargo.toml")).expect("read root Cargo.toml");
    assert_eq!(
        root_toml_before, root_toml_after,
        "root Cargo.toml must be untouched on validation failure"
    );
}
