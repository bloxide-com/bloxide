// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for `generate_system_wiring_from_toml` and
//! `generate_cargo_toml` (issue #130) — the two most complex functions in
//! the codegen, previously with zero test coverage.
//!
//! Strategy: run both against the REAL workspace manifests
//! (`apps/*/system.toml`, workspace root `../../`) and assert structural
//! properties of the generated main.rs / Cargo.toml — channel creation,
//! supervisor setup, injection wiring, bootstrap, runtime selection,
//! dynamic-actor handling, dependency resolution, and feature inference.
//! The generated app `src/generated/` skeletons these functions write are
//! gitignored, so the side effects are harmless.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    // crates/tools/bloxide-codegen → repo root
    PathBuf::from("../../../")
}

fn wiring(app: &str) -> String {
    let system_path = workspace_root().join(app).join("system.toml");
    bloxide_codegen::generate_system_wiring_from_toml(&system_path, &workspace_root())
        .unwrap_or_else(|e| panic!("wiring generation failed for {}: {}", app, e))
}

fn cargo_toml(app: &str) -> String {
    let system_path = workspace_root().join(app).join("system.toml");
    bloxide_codegen::generate_cargo_toml(&system_path, &workspace_root())
        .unwrap_or_else(|e| panic!("Cargo.toml generation failed for {}: {}", app, e))
}

fn assert_parses_rust(app: &str, code: &str) {
    syn::parse_str::<syn::File>(code)
        .unwrap_or_else(|e| panic!("generated main.rs for {} is not valid Rust: {}", app, e));
}

// ── system wiring: tokio-demo (ping/pong, supervision) ─────────────────────

#[test]
fn wiring_tokio_demo_parses_and_selects_tokio_runtime() {
    let main_rs = wiring("apps/tokio-demo");
    assert_parses_rust("tokio-demo", &main_rs);
    assert!(
        main_rs.contains("bloxide_tokio") || main_rs.contains("TokioRuntime"),
        "tokio-demo main.rs should select the Tokio runtime"
    );
    assert!(
        main_rs.contains("#[tokio::main]"),
        "tokio-demo main.rs should use #[tokio::main]"
    );
}

#[test]
fn wiring_tokio_demo_creates_channels_and_supervisor() {
    let main_rs = wiring("apps/tokio-demo");
    assert!(
        main_rs.contains("bloxide_supervisor_spec_skeleton"),
        "should reference the generated supervisor spec"
    );
    assert!(
        main_rs.contains("SupervisorCtx"),
        "should construct the supervisor context"
    );
    assert!(
        main_rs.contains("LifecycleCommand::Start"),
        "should dispatch Start at bootstrap"
    );
}

#[test]
fn wiring_tokio_demo_wires_peer_injection() {
    let main_rs = wiring("apps/tokio-demo");
    // ping's peer_ref comes from pong (and vice versa): pong's channel ref is
    // cloned into PingCtx as a constructor argument (multi-line form), and
    // ping's ref goes into PongCtx (single-line form).
    assert!(
        main_rs.contains("let ping_ctx = PingCtx::new("),
        "ping context should be constructed"
    );
    assert!(
        main_rs.contains("pong_ref.clone()"),
        "pong's channel ref should be injected into ping's context"
    );
    assert!(
        main_rs.contains("PongCtx::new(pong_id, ping_ref.clone()"),
        "ping's channel ref should be injected into pong's context"
    );
}

// ── system wiring: tokio-minimal-demo (single actor) ───────────────────────

#[test]
fn wiring_minimal_demo_parses_and_builds_counter() {
    let main_rs = wiring("apps/tokio-minimal-demo");
    assert_parses_rust("tokio-minimal-demo", &main_rs);
    assert!(
        main_rs.contains("counter_spec_skeleton"),
        "should reference the generated counter spec"
    );
    assert!(
        main_rs.contains("CounterMsg::Tick"),
        "bootstrap Tick messages should be emitted"
    );
}

// ── system wiring: tokio-pool-demo (factory injection, dynamic actor) ──────

#[test]
fn wiring_pool_demo_injects_spawn_factory() {
    let main_rs = wiring("apps/tokio-pool-demo");
    assert_parses_rust("tokio-pool-demo", &main_rs);
    assert!(
        main_rs.contains("::tokio_pool_demo_impl::spawn_worker"),
        "the impl crate's spawn function should be injected"
    );
    // The factory is monomorphized with the system-level concrete worker spec.
    assert!(
        main_rs.contains("worker_spec_skeleton::WorkerSpec"),
        "spawn factory should be monomorphized with the generated worker spec"
    );
}

#[test]
fn wiring_pool_demo_dynamic_actor_gets_spec_but_no_static_task() {
    let main_rs = wiring("apps/tokio-pool-demo");
    assert_parses_rust("tokio-pool-demo", &main_rs);
    // The dynamic worker gets a concrete spec at system level…
    assert!(
        main_rs.contains("worker_spec_skeleton"),
        "dynamic worker should have a generated concrete spec"
    );
    // …but no statically spawned task for it (spawning happens at runtime
    // through the factory, not in main.rs).
    assert!(
        !main_rs.contains("worker_task!"),
        "dynamic worker must not get a static task macro"
    );
}

#[test]
fn wiring_pool_demo_multi_mailbox_actor() {
    let main_rs = wiring("apps/tokio-pool-demo");
    // Pool has multiple mailboxes (domain + spawn-reply + peer control).
    // The channels! call should create more than one channel for the pool.
    assert!(
        main_rs.contains("SpawnedWorker") || main_rs.contains("spawn_reply"),
        "pool's secondary (spawn-reply) mailbox should appear in wiring"
    );
}

// ── system wiring: embassy-demo (embassy runtime selection) ────────────────

#[test]
fn wiring_embassy_demo_selects_embassy_runtime() {
    let main_rs = wiring("apps/embassy-demo");
    assert_parses_rust("embassy-demo", &main_rs);
    assert!(
        main_rs.contains("bloxide_embassy") || main_rs.contains("embassy_executor"),
        "embassy-demo main.rs should select the Embassy runtime"
    );
    assert!(
        !main_rs.contains("#[tokio::main]"),
        "embassy-demo must not use #[tokio::main]"
    );
}

// ── generate_cargo_toml: dependency resolution + features ──────────────────

#[test]
fn cargo_toml_tokio_demo_resolves_dependencies() {
    let out = cargo_toml("apps/tokio-demo");
    for dep in [
        "bloxide-core",
        "bloxide-tokio",
        "tokio",
        "ping-blox",
        "pong-blox",
        "ping-pong-messages",
        "blox-ctx-ping-pong",
        "bloxide-supervisor",
        "bloxide-child-management",
    ] {
        assert!(
            out.contains(dep),
            "tokio-demo Cargo.toml missing dep {}",
            dep
        );
    }
    assert!(
        out.contains("license.workspace = true"),
        "app Cargo.toml must inherit the workspace license"
    );
}

#[test]
fn cargo_toml_pool_demo_infers_dynamic_feature() {
    let out = cargo_toml("apps/tokio-pool-demo");
    // Factory injection (source = "factory") must infer the `dynamic`
    // feature on the affected deps.
    assert!(
        out.contains("pool-blox = { workspace = true, features = [\"dynamic\""),
        "pool-blox should carry the inferred dynamic feature"
    );
    assert!(
        out.contains("tokio-pool-demo-impl = { workspace = true, features = [\"dynamic\"] }"),
        "impl crate should carry the inferred dynamic feature"
    );
    assert!(
        out.contains("tokio-pool-demo-impl"),
        "pool demo must depend on the impl crate (which wraps bloxide-spawn)"
    );
}

#[test]
fn cargo_toml_embassy_demo_uses_embassy_deps() {
    let out = cargo_toml("apps/embassy-demo");
    assert!(
        out.contains("bloxide-embassy"),
        "embassy-demo must depend on bloxide-embassy"
    );
    // Embassy crates are NOT workspace deps — they get inline version specs.
    assert!(
        out.contains("embassy-executor = { version ="),
        "embassy-executor should be an inline version dep, got:\n{}",
        out
    );
    assert!(
        !out.contains("bloxide-tokio"),
        "embassy-demo must not depend on bloxide-tokio"
    );
}

#[test]
fn cargo_toml_minimal_demo_discovers_message_crates() {
    let out = cargo_toml("apps/tokio-minimal-demo");
    for dep in ["counter-messages", "blox-ctx-ticks", "counter-blox"] {
        assert!(
            out.contains(dep),
            "tokio-minimal-demo Cargo.toml missing dep {}",
            dep
        );
    }
}
