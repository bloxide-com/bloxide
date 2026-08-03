// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the `cargo-blox add-supervision` and `set-policy`
//! commands: strategy validation and supervision-group targeting.
//!
//! Each test creates a temporary workspace with an `apps/<app>/system.toml`
//! fixture, spawns the `cargo-blox` binary as a subprocess, and reads back
//! the system.toml to verify the edit.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the compiled `cargo-blox` binary.
fn blox_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-blox"))
}

/// A system.toml with two actors and no supervision sections.
const SYSTEM_NO_SUPERVISION: &str = "\
[system]
runtime = \"tokio\"

[[actors]]
name = \"ping\"
blox = \"ping-blox\"

[[actors]]
name = \"worker\"
blox = \"worker-blox\"
";

/// A system.toml with TWO supervision groups: group A supervises `ping`,
/// group B supervises `worker`.
const SYSTEM_TWO_GROUPS: &str = "\
[system]
runtime = \"tokio\"

[[actors]]
name = \"ping\"
blox = \"ping-blox\"

[[actors]]
name = \"worker\"
blox = \"worker-blox\"

[[supervision]]
supervisor = \"sup-a\"
strategy = \"when_any_done\"
children = [\"ping\"]

[[supervision]]
supervisor = \"sup-b\"
strategy = \"when_all_done\"
children = [\"worker\"]
";

/// Writes a system.toml fixture to `<temp>/apps/<app>/system.toml`.
fn write_fixture(app: &str, content: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let app_dir = dir.path().join("apps").join(app);
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::write(app_dir.join("system.toml"), content).expect("write system.toml");
    dir
}

/// Runs `cargo-blox blox <args...>` in `dir` and returns
/// (stdout, stderr, exit code).
fn run_blox(dir: &TempDir, args: &[&str]) -> (String, String, i32) {
    let mut cmd = Command::new(blox_bin());
    cmd.current_dir(dir.path());
    cmd.arg("blox");
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().expect("spawn cargo-blox");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().expect("exit code present"),
    )
}

/// Reads back the system.toml as a parsed `toml::Value`.
fn read_back_system(dir: &TempDir, app: &str) -> toml::Value {
    let path = dir.path().join("apps").join(app).join("system.toml");
    let content = fs::read_to_string(&path).expect("read system.toml back");
    toml::from_str(&content).expect("parse system.toml back")
}

/// Returns the `[[supervision]]` entry whose children list contains `actor`.
fn supervision_group_for<'a>(doc: &'a toml::Value, actor: &str) -> Option<&'a toml::Value> {
    doc.get("supervision")?.as_array()?.iter().find(|entry| {
        entry
            .get("children")
            .and_then(|c| c.as_array())
            .map(|children| children.iter().any(|a| a.as_str() == Some(actor)))
            .unwrap_or(false)
    })
}

// ---------------------------------------------------------------------------
// Strategy validation: unknown strategies are rejected before any edit.
// ---------------------------------------------------------------------------

#[test]
fn unknown_strategy_rejected() {
    let dir = write_fixture("demo", SYSTEM_NO_SUPERVISION);
    let (_stdout, stderr, code) = run_blox(
        &dir,
        &[
            "add-supervision",
            "demo",
            "--supervisor",
            "sup",
            "--strategy",
            "one_for_one",
            "--child",
            "ping",
        ],
    );
    assert_ne!(code, 0, "unknown strategy should fail");
    assert!(
        stderr.contains("unknown strategy 'one_for_one'"),
        "stderr should name the bad strategy: {stderr}"
    );
    assert!(
        stderr.contains("when_any_done") && stderr.contains("when_all_done"),
        "stderr should list the valid strategies: {stderr}"
    );

    // No supervision section was written.
    let doc = read_back_system(&dir, "demo");
    assert!(
        doc.get("supervision").is_none(),
        "failed add-supervision must not edit system.toml"
    );
}

#[test]
fn when_all_done_accepted() {
    let dir = write_fixture("demo", SYSTEM_NO_SUPERVISION);
    let (_stdout, _stderr, code) = run_blox(
        &dir,
        &[
            "add-supervision",
            "demo",
            "--supervisor",
            "sup",
            "--strategy",
            "when_all_done",
            "--child",
            "ping",
            "--child",
            "worker",
        ],
    );
    assert_eq!(code, 0, "when_all_done should be accepted");

    let doc = read_back_system(&dir, "demo");
    let sup = &doc["supervision"][0];
    assert_eq!(sup["strategy"].as_str(), Some("when_all_done"));
    let children: Vec<&str> = sup["children"]
        .as_array()
        .expect("children is array")
        .iter()
        .map(|c| c.as_str().expect("child is string"))
        .collect();
    assert_eq!(children, vec!["ping", "worker"]);
}

#[test]
fn when_any_done_accepted() {
    let dir = write_fixture("demo", SYSTEM_NO_SUPERVISION);
    let (_stdout, _stderr, code) = run_blox(
        &dir,
        &[
            "add-supervision",
            "demo",
            "--supervisor",
            "sup",
            "--strategy",
            "when_any_done",
            "--child",
            "ping",
        ],
    );
    assert_eq!(code, 0, "when_any_done should be accepted");

    let doc = read_back_system(&dir, "demo");
    assert_eq!(
        doc["supervision"][0]["strategy"].as_str(),
        Some("when_any_done")
    );
}

// ---------------------------------------------------------------------------
// set-policy targets the supervision group whose children list contains the
// actor — not blindly the first group.
// ---------------------------------------------------------------------------

#[test]
fn set_policy_targets_group_containing_actor() {
    let dir = write_fixture("demo", SYSTEM_TWO_GROUPS);
    let (_stdout, _stderr, code) = run_blox(
        &dir,
        &[
            "set-policy",
            "demo",
            "--actor",
            "worker",
            "--restart-max",
            "3",
        ],
    );
    assert_eq!(code, 0, "set-policy should succeed");

    let doc = read_back_system(&dir, "demo");

    // The policy landed in group B (the one supervising `worker`)...
    let group_b = supervision_group_for(&doc, "worker").expect("group B exists");
    assert_eq!(group_b["supervisor"].as_str(), Some("sup-b"));
    let policy = &group_b["policies"]["worker"];
    assert_eq!(policy["restart"]["max"].as_integer(), Some(3));

    // ...and NOT in group A.
    let group_a = supervision_group_for(&doc, "ping").expect("group A exists");
    let a_policies = group_a.get("policies");
    assert!(
        a_policies.is_none() || a_policies.unwrap().get("worker").is_none(),
        "group A must not gain a policy for worker: {group_a}"
    );
}

#[test]
fn set_policy_actor_in_first_group() {
    let dir = write_fixture("demo", SYSTEM_TWO_GROUPS);
    let (_stdout, _stderr, code) =
        run_blox(&dir, &["set-policy", "demo", "--actor", "ping", "--stop"]);
    assert_eq!(code, 0, "set-policy should succeed");

    let doc = read_back_system(&dir, "demo");
    let group_a = supervision_group_for(&doc, "ping").expect("group A exists");
    assert_eq!(group_a["policies"]["ping"]["stop"].as_bool(), Some(true));

    let group_b = supervision_group_for(&doc, "worker").expect("group B exists");
    assert!(
        group_b.get("policies").is_none(),
        "group B must not gain policies: {group_b}"
    );
}

#[test]
fn set_policy_actor_not_in_any_group_fails() {
    let dir = write_fixture("demo", SYSTEM_TWO_GROUPS);
    let (_stdout, stderr, code) =
        run_blox(&dir, &["set-policy", "demo", "--actor", "ghost", "--stop"]);
    assert_ne!(code, 0, "set-policy for an unsupervised actor should fail");
    assert!(
        stderr.contains("ghost"),
        "stderr should name the actor: {stderr}"
    );
}
