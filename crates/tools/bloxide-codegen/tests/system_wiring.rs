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

// ── system wiring: tokio-minimal-demo (single actor) ────────────────────────

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

// ── system wiring: embassy-demo (embassy runtime selection) ───────────────

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

// ── supervision policy + capacities emission ───────────────────────────────

fn tokio_demo_manifest() -> String {
    let path = workspace_root().join("apps/tokio-demo/system.toml");
    std::fs::read_to_string(path).expect("read tokio-demo manifest")
}

fn wiring_from_manifest(manifest: &str, tag: &str) -> Result<String, anyhow::Error> {
    let dir = std::env::temp_dir().join(format!("bloxide-codegen-test-{}", tag));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let system_path = dir.join("system.toml");
    std::fs::write(&system_path, manifest).expect("write temp system.toml");
    bloxide_codegen::generate_system_wiring_from_toml(&system_path, &workspace_root())
}

#[test]
fn wiring_emits_restart_max_and_default_capacities() {
    let manifest =
        tokio_demo_manifest().replace("ping = { stop = true }", "ping = { restart = { max = 3 } }");
    let main_rs = wiring_from_manifest(&manifest, "restart-default")
        .unwrap_or_else(|e| panic!("wiring failed: {}", e));
    assert!(
        main_rs.contains("ChildPolicy::Reset { max: 3u32 }"),
        "restart.max must be carried into ChildPolicy::Reset, got:\n{}",
        main_rs
    );
    // Defaults: notify = max(32, 2 × 2 children) = 32, control 16, lifecycle 4.
    assert!(
        main_rs.contains("ChildGroupBuilder::<_, _, 32usize, 16usize, 4usize>::new"),
        "default capacities must be emitted as const generics, got:\n{}",
        main_rs
    );
}

#[test]
fn wiring_emits_explicit_capacities() {
    let manifest = tokio_demo_manifest()
        + "\n  [supervision.capacities]\n  notify = 64\n  control = 24\n  lifecycle = 8\n";
    let main_rs = wiring_from_manifest(&manifest, "capacities")
        .unwrap_or_else(|e| panic!("wiring failed: {}", e));
    assert!(
        main_rs.contains("ChildGroupBuilder::<_, _, 64usize, 24usize, 8usize>::new"),
        "explicit [supervision.capacities] must be emitted, got:\n{}",
        main_rs
    );
}

#[test]
fn wiring_rejects_zero_restart_max() {
    let manifest =
        tokio_demo_manifest().replace("ping = { stop = true }", "ping = { restart = { max = 0 } }");
    let err = wiring_from_manifest(&manifest, "max-zero")
        .expect_err("restart.max = 0 must be a hard error");
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("restart.max"),
        "error should name restart.max, got: {}",
        msg
    );
}

#[test]
fn wiring_rejects_zero_capacity() {
    let manifest = tokio_demo_manifest() + "\n  [supervision.capacities]\n  notify = 0\n";
    let err = wiring_from_manifest(&manifest, "capacity-zero")
        .expect_err("capacity 0 must be a hard error");
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("capacities.notify"),
        "error should name capacities.notify, got: {}",
        msg
    );
}

// ── Watchdog driver + max_misses emission ────────────────────────────────

#[test]
fn wiring_emits_watchdog_driver_for_tokio() {
    let manifest = tokio_demo_manifest()
        + "\n  [supervision.watchdog]\n  interval_ms = 500\n  max_misses = 3\n";
    let main_rs = wiring_from_manifest(&manifest, "watchdog-tokio")
        .unwrap_or_else(|e| panic!("wiring failed: {}", e));
    assert_parses_rust("watchdog-tokio", &main_rs);
    assert!(
        main_rs.contains("tokio::time::interval"),
        "tokio watchdog driver must use tokio::time::interval, got:\n{}",
        main_rs
    );
    assert!(
        main_rs.contains("ChildCtrl::WatchdogTick"),
        "watchdog driver must send WatchdogTick, got:\n{}",
        main_rs
    );
    assert!(
        main_rs.contains("ChildGroupBuilder::<_, _, 32usize, 16usize, 4usize>::new(GroupShutdown::WhenAnyDone, 3)"),
        "max_misses must be passed to ChildGroupBuilder, got:\n{}",
        main_rs
    );
}

#[test]
fn wiring_emits_watchdog_driver_for_embassy() {
    let manifest = std::fs::read_to_string(workspace_root().join("apps/embassy-demo/system.toml"))
        .expect("read embassy-demo manifest")
        + "\n  [supervision.watchdog]\n  interval_ms = 1000\n";
    let main_rs = wiring_from_manifest(&manifest, "watchdog-embassy")
        .unwrap_or_else(|e| panic!("wiring failed: {}", e));
    assert_parses_rust("watchdog-embassy", &main_rs);
    assert!(
        main_rs.contains("#[embassy_executor::task]")
            && main_rs.contains("async fn watchdog_task")
            && main_rs.contains("embassy_time::Timer::after_millis"),
        "embassy watchdog driver must be a task using embassy_time::Timer, got:\n{}",
        main_rs
    );
    assert!(
        main_rs.contains("spawner.must_spawn(watchdog_task("),
        "embassy watchdog task must be spawned, got:\n{}",
        main_rs
    );
    assert!(
        main_rs.contains("ChildGroupBuilder::<_, _, 32usize, 16usize, 4usize>::new(GroupShutdown::WhenAnyDone, 2)"),
        "default max_misses (2) must be passed to ChildGroupBuilder, got:\n{}",
        main_rs
    );
}

#[test]
fn wiring_rejects_zero_watchdog_interval() {
    let manifest = tokio_demo_manifest() + "\n  [supervision.watchdog]\n  interval_ms = 0\n";
    let err = wiring_from_manifest(&manifest, "watchdog-zero")
        .expect_err("watchdog interval_ms = 0 must be a hard error");
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("interval_ms"),
        "error should name interval_ms, got: {}",
        msg
    );
}

#[test]
fn wiring_rejects_stop_false() {
    let manifest =
        tokio_demo_manifest().replace("ping = { stop = true }", "ping = { stop = false }");
    let err = wiring_from_manifest(&manifest, "stop-false")
        .expect_err("stop = false must be a hard error");
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("stop = false"),
        "error should name stop = false, got: {}",
        msg
    );
}
