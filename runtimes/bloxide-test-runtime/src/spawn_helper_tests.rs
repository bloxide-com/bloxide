// Copyright 2025 Bloxide, all rights reserved
//! Tests for the `spawn_child` helper and `ChildCtrlRegistrar` bridge.
//!
//! These live in `bloxide-test-runtime` (not in `bloxide-spawn`) for the same
//! reason the engine lifecycle tests do: a `bloxide-spawn` dev-dependency on
//! this crate would create a dev-dependency cycle, giving two instances of
//! `bloxide-spawn` in the dependency graph whose `SpawnCap` traits don't
//! unify.

use bloxide_child_management::control::ChildCtrl;
use bloxide_child_management::ChildPolicy;
use bloxide_core::capability::DynamicChannelCap;
use bloxide_core::lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::{ActorId, ActorRef};
use bloxide_spawn::{spawn_child, ChildCtrlRegistrar, SpawnCap, SpawnOutput};

use crate::TestRuntime;

#[derive(Clone)]
struct DummyReq;

/// A spawn function that creates real TestRuntime channels and records a
/// spawned (unexecuted) task, returning its spawn id as the kill handle.
fn dummy_spawn(
    _req: DummyReq,
    _notify: ActorRef<ChildLifecycleEvent, TestRuntime>,
) -> SpawnOutput<TestRuntime> {
    let child_id = TestRuntime::alloc_actor_id();
    let (lifecycle_ref, _lc_rx) = TestRuntime::channel::<LifecycleCommand>(child_id, 4);
    let (abort_ref, _abort_rx) = TestRuntime::channel::<AbortCommand>(child_id + 1, 4);
    let task = TestRuntime::spawn(async {});
    let kill_handle = TestRuntime::kill_handle(task);
    SpawnOutput {
        child_id,
        lifecycle_ref,
        abort_ref,
        kill_handle,
        policy: ChildPolicy::Stop,
    }
}

fn setup(
    control_capacity: usize,
) -> (
    ActorRef<ChildCtrl<TestRuntime>, TestRuntime>,
    ActorRef<ChildLifecycleEvent, TestRuntime>,
    crate::TestReceiver<ChildCtrl<TestRuntime>>,
    crate::TestReceiver<ChildLifecycleEvent>,
) {
    // The receivers are returned to the caller: dropping them would close the
    // send side (TestRuntime models receiver-drop as Closed).
    let (control_ref, control_rx) =
        TestRuntime::channel::<ChildCtrl<TestRuntime>>(1, control_capacity);
    let (notify_ref, notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(2, 16);
    (control_ref, notify_ref, control_rx, notify_rx)
}

#[test]
fn registration_send_failure_kills_orphaned_task() {
    let (control_ref, notify_ref, _control_rx, _notify_rx) = setup(0); // capacity 0 — always Full
    crate::drain_killed();

    let result = spawn_child::<TestRuntime, DummyReq, ChildCtrlRegistrar>(
        dummy_spawn,
        DummyReq,
        &control_ref,
        &notify_ref,
        99 as ActorId,
    );

    assert!(
        result.is_err(),
        "full control channel must fail the spawn_child call"
    );
    let killed = crate::drain_killed();
    assert_eq!(
        killed.len(),
        1,
        "the orphaned task must be killed when registration fails"
    );
}

#[test]
fn successful_registration_does_not_kill() {
    let (control_ref, notify_ref, _control_rx, _notify_rx) = setup(1);
    crate::drain_killed();

    let result = spawn_child::<TestRuntime, DummyReq, ChildCtrlRegistrar>(
        dummy_spawn,
        DummyReq,
        &control_ref,
        &notify_ref,
        99 as ActorId,
    );

    assert!(result.is_ok());
    assert_eq!(crate::kill_count(), 0, "no kill may fire on the happy path");
}
