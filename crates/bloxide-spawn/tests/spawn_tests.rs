// Copyright 2025 Bloxide, all rights reserved
//! Direct tests for `bloxide-spawn`: `ChildCtrlRegistrar`, `spawn_dynamic_child`,
//! `Kill`, `SpawnOutput`, and the platform spawn API (`ActorParts` +
//! `spawn_actor_task`).
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
use bloxide_core::capability::{BloxRuntime, DynamicChannelCap};
use bloxide_core::engine::StateMachine;
use bloxide_core::event_tag::{EventTag, LifecycleEvent};
use bloxide_core::lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::{ActorId, ActorRef, Envelope};
use bloxide_core::spec::{MachineSpec, StateFns};
use bloxide_core::topology::StateTopology;
use bloxide_spawn::{
    spawn_actor_task, spawn_dynamic_child, ActorParts, ChildCtrlRegistrar, ChildRegistrar, Kill,
    KillCapability, SpawnCap, SpawnFn, SpawnOutput,
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

// ── ActorParts + spawn_actor_task ───────────────────────────────────────────
//
// The fixture is the same shape as the waker-test fixtures in
// `bloxide-test-runtime` (StateTopology + EventTag/LifecycleEvent impls +
// HANDLER_TABLE), trimmed to the smallest `MachineSpec` that
// `StateMachine::new`'s invariant asserts accept: two states, no transitions,
// one domain mailbox. Same integration-test placement as the rest of this
// file (see the header comment) — it exercises `TestRuntime: SpawnCap`.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
enum FixtureState {
    #[default]
    Init,
    Running,
}

impl StateTopology for FixtureState {
    const STATE_COUNT: usize = 2;

    fn parent(self) -> Option<Self> {
        None
    }

    fn is_leaf(self) -> bool {
        true
    }

    fn path(self) -> &'static [Self] {
        match self {
            FixtureState::Init => &[FixtureState::Init],
            FixtureState::Running => &[FixtureState::Running],
        }
    }

    fn as_index(self) -> usize {
        self as usize
    }
}

// Variants are never constructed: `TestRuntime` records spawned tasks without
// executing them, so the fixture machine never dispatches. The variants exist
// to satisfy the `EventTag`/`LifecycleEvent`/`From` trait surface only.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum FixtureEvent {
    Lifecycle(LifecycleCommand),
    Msg(u32),
}

impl EventTag for FixtureEvent {
    fn event_tag(&self) -> u8 {
        match self {
            FixtureEvent::Lifecycle(_) => 254,
            FixtureEvent::Msg(_) => 0,
        }
    }
}

impl LifecycleEvent for FixtureEvent {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        match self {
            FixtureEvent::Lifecycle(cmd) => Some(*cmd),
            _ => None,
        }
    }
}

impl From<Envelope<u32>> for FixtureEvent {
    fn from(env: Envelope<u32>) -> Self {
        FixtureEvent::Msg(env.1)
    }
}

/// Unit context — the fixture machine carries no state.
struct FixtureCtx;

/// Minimal spec: two states (Init → Running), no transitions, a single u32
/// domain mailbox.
struct FixtureSpec;

impl MachineSpec for FixtureSpec {
    type State = FixtureState;
    type Event = FixtureEvent;
    type Ctx = FixtureCtx;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<u32>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = &[
        &StateFns {
            on_entry: &[],
            on_exit: &[],
            transitions: &[],
        },
        &StateFns {
            on_entry: &[],
            on_exit: &[],
            transitions: &[],
        },
    ];

    fn initial_state() -> FixtureState {
        FixtureState::Running
    }
}

/// The domain-factory half of the codegen composition: builds everything the
/// platform spawn needs from the request — child id, machine, domain and
/// lifecycle/abort channels — without touching `run()`, `RunConfig`, or
/// `SpawnCap`. All channels share the child id, mirroring handwritten spawn
/// factories (e.g. the pool demo's `build_worker`).
fn build_parts(_req: DummyReq) -> ActorParts<FixtureSpec, TestRuntime> {
    let child_id = TestRuntime::alloc_actor_id();
    // In a real factory the domain send-side goes back to the requester via
    // the spawn request's reply channel; the spawn path itself never sees it.
    let (_domain_ref, domain_rx) = TestRuntime::channel::<u32>(child_id, 4);
    let (lifecycle_ref, lifecycle_rx) = TestRuntime::channel::<LifecycleCommand>(child_id, 4);
    let (abort_ref, abort_rx) = TestRuntime::channel::<AbortCommand>(child_id, 4);
    ActorParts {
        child_id,
        machine: StateMachine::<FixtureSpec>::new(FixtureCtx),
        mailboxes: (TestRuntime::to_stream(domain_rx),),
        lifecycle_ref,
        lifecycle_rx,
        abort_ref,
        abort_rx,
        policy: ChildPolicy::Stop,
    }
}

#[test]
fn spawn_actor_task_spawns_run_loop_and_returns_output() {
    let spawned_before = spawned_count();
    let _ = drain_killed();
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(2, 16);

    let parts = build_parts(DummyReq);
    let child_id = parts.child_id;
    let output = spawn_actor_task(parts, notify_ref);

    assert_eq!(
        spawned_count(),
        spawned_before + 1,
        "exactly one run-loop task must be spawned"
    );
    assert_eq!(output.child_id, child_id);
    assert_eq!(output.lifecycle_ref.id(), child_id);
    assert_eq!(output.abort_ref.id(), child_id);
    assert_eq!(output.policy, ChildPolicy::Stop);

    // The kill handle is the ripcord for the spawned task: killing via the
    // `Kill` capability must record exactly this handle.
    <Kill as KillCapability<TestRuntime>>::kill(output.kill_handle);
    assert_eq!(drain_killed(), vec![output.kill_handle]);
}

#[test]
fn codegen_composition_flows_through_spawn_dynamic_child() {
    let (control_ref, mut control_rx) = TestRuntime::channel::<ChildCtrl<TestRuntime>>(1, 4);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(2, 16);
    let spawned_before = spawned_count();

    // The exact shape system codegen emits at the wiring layer: a stateless
    // closure composing the domain factory with the platform spawn, coerced
    // to the `SpawnFn` fn-pointer type.
    let spawn_fn: SpawnFn<TestRuntime, DummyReq> =
        |req, notify| spawn_actor_task(build_parts(req), notify);

    let result = spawn_dynamic_child::<TestRuntime, DummyReq, ChildCtrlRegistrar>(
        spawn_fn,
        DummyReq,
        &control_ref,
        &notify_ref,
        FROM,
    );

    assert!(result.is_ok(), "spawn_dynamic_child failed: {result:?}");
    assert_eq!(
        spawned_count(),
        spawned_before + 1,
        "the platform spawn must record exactly one run-loop task"
    );

    let msgs = control_rx.drain_payloads();
    assert_eq!(msgs.len(), 1, "exactly one registration message expected");
    match &msgs[0] {
        ChildCtrl::RegisterDynamicChild(reg) => {
            assert_eq!(reg.policy, ChildPolicy::Stop);
            assert_eq!(reg.lifecycle_ref.id(), reg.id);
            assert_eq!(reg.abort_ref.id(), reg.id);
        }
        other => panic!("expected RegisterDynamicChild, got {other:?}"),
    }
}
