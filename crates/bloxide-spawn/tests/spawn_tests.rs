// Copyright 2025 Bloxide, all rights reserved
//! Direct tests for `bloxide-spawn`: `ChildCtrlRegistrar`, `spawn_dynamic_child`,
//! `Kill`, and `SpawnOutput`.
//!
//! These are integration tests (not a `#[cfg(test)]` module in `src/`) on purpose:
//! `bloxide-test-runtime` depends on `bloxide-spawn`, so the dev-dependency below
//! closes a cycle. Cargo supports that cycle for integration tests — every crate
//! in the graph links the single normal `bloxide-spawn` lib unit — but unit tests
//! would compile the crate a second time with `cfg(test)`, and that instance's
//! `SpawnCap` trait would not unify with the one `TestRuntime` implements
//! ("multiple different versions of crate `bloxide_spawn` in the dependency
//! graph"). `cargo test -p bloxide-spawn` runs this file the same either way.
//!
//! `TestRuntime` records (not executes) spawned tasks and kills; its spawn/kill
//! logs are thread-local, and the test harness runs each test on its own thread,
//! so tests are isolated from one another.

use bloxide_child_management::control::ChildCtrl;
use bloxide_child_management::ChildPolicy;
use bloxide_core::capability::DynamicChannelCap;
use bloxide_core::lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::{ActorId, ActorRef};
use bloxide_spawn::{
    spawn_dynamic_child, ChildCtrlRegistrar, ChildRegistrar, Kill, KillCapability, SpawnCap,
    SpawnOutput,
};
use bloxide_test_runtime::{
    drain_killed, kill_count, spawned_count, TestRuntime, TestTrySendError,
};

use std::cell::Cell;

/// Sender id stamped on registration envelopes (stands in for the requesting blox).
const FROM: ActorId = 99;

struct DummyReq;

// Records the `(child_id, kill_handle)` of the most recent `dummy_spawn` call.
// Thread-local like the runtime's spawn/kill logs, so parallel tests can't race.
std::thread_local! {
    static LAST_SPAWN: Cell<Option<(ActorId, usize)>> = const { Cell::new(None) };
}

/// A spawn function that creates real TestRuntime channels and records a spawned
/// (unexecuted) task, returning its spawn id as the kill handle — the same shape
/// an app's real spawn function has, minus the domain mailboxes.
fn dummy_spawn(
    _req: DummyReq,
    _notify: ActorRef<ChildLifecycleEvent, TestRuntime>,
) -> SpawnOutput<TestRuntime> {
    let child_id = TestRuntime::alloc_actor_id();
    let (lifecycle_ref, _lc_rx) = TestRuntime::channel::<LifecycleCommand>(child_id, 4);
    let (abort_ref, _abort_rx) = TestRuntime::channel::<AbortCommand>(child_id + 1, 4);
    let task = TestRuntime::spawn(async {});
    let kill_handle = TestRuntime::kill_handle(task);
    LAST_SPAWN.with(|s| s.set(Some((child_id, kill_handle))));
    SpawnOutput {
        child_id,
        lifecycle_ref,
        abort_ref,
        kill_handle,
        policy: ChildPolicy::Stop,
    }
}

// ── ChildCtrlRegistrar ────────────────────────────────────────────────────

#[test]
fn child_ctrl_registrar_wraps_spawn_output() {
    let child_id = TestRuntime::alloc_actor_id();
    let (lifecycle_ref, _lc_rx) = TestRuntime::channel::<LifecycleCommand>(child_id, 4);
    let (abort_ref, _abort_rx) = TestRuntime::channel::<AbortCommand>(child_id + 1, 4);
    let task = TestRuntime::spawn(async {});
    let kill_handle = TestRuntime::kill_handle(task);
    let output = SpawnOutput::<TestRuntime> {
        child_id,
        lifecycle_ref,
        abort_ref,
        kill_handle,
        policy: ChildPolicy::Kill,
    };

    let msg = ChildCtrlRegistrar::register(output);

    match msg {
        ChildCtrl::RegisterDynamicChild(reg) => {
            assert_eq!(reg.id, child_id);
            assert_eq!(reg.lifecycle_ref.id(), child_id);
            assert_eq!(reg.abort_ref.id(), child_id + 1);
            assert_eq!(reg.kill_handle, kill_handle);
            assert_eq!(reg.policy, ChildPolicy::Kill);
        }
        other => panic!("expected RegisterDynamicChild, got {other:?}"),
    }
}

// ── spawn_dynamic_child ───────────────────────────────────────────────────

#[test]
fn spawn_dynamic_child_sends_registration_and_returns_ok() {
    let (control_ref, mut control_rx) = TestRuntime::channel::<ChildCtrl<TestRuntime>>(1, 4);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(2, 16);

    let result = spawn_dynamic_child::<TestRuntime, DummyReq, ChildCtrlRegistrar>(
        dummy_spawn,
        DummyReq,
        &control_ref,
        &notify_ref,
        FROM,
    );

    assert!(result.is_ok(), "spawn_dynamic_child failed: {result:?}");
    let (child_id, kill_handle) = LAST_SPAWN
        .with(|s| s.get())
        .expect("spawn function must run");

    let msgs = control_rx.drain_payloads();
    assert_eq!(msgs.len(), 1, "exactly one registration message expected");
    match &msgs[0] {
        ChildCtrl::RegisterDynamicChild(reg) => {
            assert_eq!(reg.id, child_id);
            assert_eq!(reg.kill_handle, kill_handle);
            assert_eq!(reg.policy, ChildPolicy::Stop);
        }
        other => panic!("expected RegisterDynamicChild, got {other:?}"),
    }

    assert_eq!(kill_count(), 0, "no kill may fire on the happy path");
}

#[test]
fn full_control_channel_kills_spawned_task_and_returns_err() {
    // Capacity 0: every try_send fails with Full.
    let (control_ref, _control_rx) = TestRuntime::channel::<ChildCtrl<TestRuntime>>(1, 0);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(2, 16);
    let _ = drain_killed();
    let spawned_before = spawned_count();

    let result = spawn_dynamic_child::<TestRuntime, DummyReq, ChildCtrlRegistrar>(
        dummy_spawn,
        DummyReq,
        &control_ref,
        &notify_ref,
        FROM,
    );

    assert_eq!(result, Err(TestTrySendError::Full));
    let (_child_id, kill_handle) = LAST_SPAWN
        .with(|s| s.get())
        .expect("spawn function must run");
    assert_eq!(
        spawned_count(),
        spawned_before + 1,
        "the child task was spawned before registration failed"
    );
    assert_eq!(
        drain_killed(),
        vec![kill_handle],
        "the orphaned task must be killed via its ripcord when registration fails"
    );
}

// ── Kill ──────────────────────────────────────────────────────────────────

#[test]
fn kill_capability_records_the_kill() {
    let task = TestRuntime::spawn(async {});
    let handle = TestRuntime::kill_handle(task);
    let _ = drain_killed();

    <Kill as KillCapability<TestRuntime>>::kill(handle);

    assert_eq!(drain_killed(), vec![handle]);
}

// ── SpawnOutput ───────────────────────────────────────────────────────────

#[test]
fn spawn_output_clone_and_debug() {
    let child_id = 42;
    let (lifecycle_ref, _lc_rx) = TestRuntime::channel::<LifecycleCommand>(child_id, 4);
    let (abort_ref, _abort_rx) = TestRuntime::channel::<AbortCommand>(child_id + 1, 4);
    let task = TestRuntime::spawn(async {});
    let kill_handle = TestRuntime::kill_handle(task);
    let output = SpawnOutput::<TestRuntime> {
        child_id,
        lifecycle_ref,
        abort_ref,
        kill_handle,
        policy: ChildPolicy::Reset { max: 3 },
    };

    let cloned = output.clone();
    assert_eq!(cloned.child_id, output.child_id);
    assert_eq!(cloned.lifecycle_ref.id(), output.lifecycle_ref.id());
    assert_eq!(cloned.abort_ref.id(), output.abort_ref.id());
    assert_eq!(cloned.kill_handle, output.kill_handle);
    assert_eq!(cloned.policy, output.policy);

    let debug = format!("{output:?}");
    assert!(debug.contains("SpawnOutput"));
    assert!(debug.contains("42"), "child_id missing from {debug}");
    assert!(debug.contains("Reset"), "policy missing from {debug}");
}
