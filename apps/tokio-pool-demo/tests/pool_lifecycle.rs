// Copyright 2025 Bloxide, all rights reserved
//! End-to-end integration test for the pool-demo supervision lifecycle,
//! exercising the SYSTEM-LEVEL concrete specs (`src/generated/`) on the real
//! Tokio runtime — the same wiring as `src/main.rs`.
//!
//! Flow under test:
//!
//! 1. A supervisor (`SupervisorSpec`) manages a child group with
//!    `GroupShutdown::WhenAllDone`.
//! 2. The pool (`PoolSpec` with real actions) runs as a supervised child.
//! 3. Two `SpawnWorker` messages are sent to the pool. The pool's real spawn
//!    factory (`tokio_pool_demo_impl::spawn_worker`) spawns real worker
//!    tasks and registers them with the supervisor via
//!    `RegisterDynamicChild`.
//! 4. The pool sends `DoWork` to each worker; the worker computes the result
//!    and replies `WorkDone`, then self-stops via `Decision::Stop` (its run
//!    loop reports `ChildLifecycleEvent::Stopped`).
//! 5. When the last `WorkDone` drains the pool's `pending` counter, the
//!    pool's guard returns `Decision::Stop` and reports `Stopped`.
//! 6. With every child stopped, `WhenAllDone` shuts the group down: the
//!    supervisor returns `Decision::Done` and its root run loop exits — the
//!    completion the test waits on.
//!
//! Observation: all child lifecycle events flow through a tap channel into a
//! small collector actor that records each event and forwards it to the
//! supervisor unchanged (the pool's `notify_ref` and the pool task's
//! `supervisor_notify` both point at the tap). The test asserts on the
//! recorded stream after the supervisor task completes — no sleeps, the
//! 10-second timeout is only a deadlock backstop.
//!
//! Run with: `cargo test -p tokio-pool-demo`

use std::sync::{Arc, Mutex};
use std::time::Duration;

use blox_ctx_pool_ref::SpawnedWorker;
use bloxide_child_management::control::ChildCtrl;
use bloxide_child_management::{ChildPolicy, GroupShutdown};
use bloxide_core::lifecycle::{ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::{ActorRef, Envelope};
use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    event_tag::{EventTag, LifecycleEvent, WILDCARD_TAG},
    spec::{MachineSpec, StateFns},
    topology::StateTopology,
    transition::{ActionResult, Decision, StateRule},
    StateMachine,
};
use bloxide_peers::PeerCtrl;
use bloxide_supervisor::{SupervisorCtx, SupervisorEvent};
use bloxide_tokio::{run, ChildGroupBuilder, RunConfig, TokioRuntime};
use pool_blox::PoolCtx;
use pool_messages::{PoolMsg, SpawnWorker, WorkerMsg};

use tokio_pool_demo::generated::bloxide_supervisor_spec_skeleton::SupervisorSpec;
use tokio_pool_demo::generated::pool_spec_skeleton::PoolSpec;
use tokio_pool_demo::generated::worker_spec_skeleton::WorkerSpec;

// ── Collector: a tap actor that records and forwards lifecycle events ────────

struct CollectorCtx {
    seen: Arc<Mutex<Vec<ChildLifecycleEvent>>>,
    forward: ActorRef<ChildLifecycleEvent, TokioRuntime>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectorState {
    Running,
}

impl StateTopology for CollectorState {
    const STATE_COUNT: usize = 1;
    fn parent(self) -> Option<Self> {
        None
    }
    fn is_leaf(self) -> bool {
        true
    }
    fn path(self) -> &'static [Self] {
        match self {
            CollectorState::Running => &[CollectorState::Running],
        }
    }
    fn as_index(self) -> usize {
        match self {
            CollectorState::Running => 0,
        }
    }
}

enum CollectorEvent {
    Msg(Envelope<ChildLifecycleEvent>),
}

impl From<Envelope<ChildLifecycleEvent>> for CollectorEvent {
    fn from(envelope: Envelope<ChildLifecycleEvent>) -> Self {
        CollectorEvent::Msg(envelope)
    }
}

impl EventTag for CollectorEvent {
    fn event_tag(&self) -> u8 {
        0
    }
}

impl LifecycleEvent for CollectorEvent {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        None
    }
}

struct CollectorSpec;

const COLLECTOR_RUNNING_FNS: StateFns<CollectorSpec> = StateFns {
    on_entry: &[],
    on_exit: &[],
    transitions: &[StateRule {
        event_tag: WILDCARD_TAG,
        matches: |__ev| matches!(__ev, CollectorEvent::Msg(_)),
        actions: &[|ctx, ev| {
            let CollectorEvent::Msg(env) = ev;
            ctx.seen
                .lock()
                .expect("collector lock poisoned")
                .push(env.1);
            ActionResult::from(ctx.forward.try_send(env.0, env.1))
        }],
        guard: |_ctx, _results, _ev| Decision::Stay,
    }],
};

impl MachineSpec for CollectorSpec {
    type State = CollectorState;
    type Event = CollectorEvent;
    type Ctx = CollectorCtx;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<ChildLifecycleEvent>,);
    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = &[&COLLECTOR_RUNNING_FNS];
    fn initial_state() -> CollectorState {
        CollectorState::Running
    }
    fn is_error(_state: &CollectorState) -> bool {
        false
    }
}

// ── Test ─────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pool_lifecycle_spawn_and_done() {
    // ── 1. Pool channels ────────────────────────────────────────────────────
    //
    // The pool has two domain mailboxes: PoolMsg (SpawnWorker / WorkDone) and
    // the SpawnedWorker reply channel fed by the spawn factory.
    let pool_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
    let (pool_ref, pool_msg_rx) =
        <TokioRuntime as DynamicChannelCap>::channel::<PoolMsg>(pool_id, 32);
    let (spawn_reply_ref, spawn_reply_rx) = <TokioRuntime as DynamicChannelCap>::channel::<
        SpawnedWorker<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>,
    >(pool_id, 32);

    // ── 2. Supervisor child group (WhenAllDone) ─────────────────────────────
    let mut group_builder: ChildGroupBuilder<TokioRuntime, ChildCtrl<TokioRuntime>> =
        ChildGroupBuilder::new(GroupShutdown::WhenAllDone);
    let sup_control_ref = group_builder.control_ref();
    let sup_notify_ref = group_builder.notify_ref();

    // ── 3. Tap channel + collector actor ────────────────────────────────────
    //
    // Children report lifecycle events to the tap; the collector records each
    // event and forwards it to the supervisor's real notify channel.
    let seen: Arc<Mutex<Vec<ChildLifecycleEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let tap_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
    let (tap_ref, tap_rx) =
        <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(tap_id, 64);
    let collector_machine = StateMachine::<CollectorSpec>::new(CollectorCtx {
        seen: seen.clone(),
        forward: sup_notify_ref.clone(),
    });
    let collector_task = tokio::spawn(run::<CollectorSpec, _, TokioRuntime>(
        collector_machine,
        (tap_rx,),
        RunConfig::<TokioRuntime>::unsupervised(),
        tap_id,
    ));

    // ── 4. Pool as a supervised child, reporting to the tap ─────────────────
    //
    // notify_ref = tap: workers spawned by the factory inherit this ref, so
    // worker lifecycle events flow through the tap as well.
    let pool_ctx = PoolCtx::new(
        pool_id,
        pool_ref.clone(),
        (|req, notify| {
            ::tokio_pool_demo_impl::spawn_worker::<WorkerSpec<TokioRuntime>>(req, notify)
        }) as _,
        sup_control_ref.clone(),
        tap_ref.clone(),
        spawn_reply_ref.clone(),
    );
    let pool_machine = StateMachine::<PoolSpec<TokioRuntime>>::new(pool_ctx);
    let (pool_lifecycle_rx, _pool_sup_notify) = group_builder.add_child(pool_id, ChildPolicy::Stop);
    tokio::spawn(run::<PoolSpec<TokioRuntime>, _, TokioRuntime>(
        pool_machine,
        (pool_msg_rx, spawn_reply_rx),
        RunConfig::<TokioRuntime>::supervised(pool_lifecycle_rx, tap_ref.sender()),
        pool_id,
    ));

    // ── 5. Supervisor machine + root run task ───────────────────────────────
    let sup_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
    let (children, sup_notify_rx, sup_control_rx) = group_builder.finish();
    let sup_ctx = SupervisorCtx::new(sup_id, children, sup_notify_ref);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    // Start the supervisor — this sends Start to all registered children (the pool).
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));
    let sup_task = tokio::spawn(run::<SupervisorSpec<TokioRuntime>, _, TokioRuntime>(
        sup_machine,
        (sup_notify_rx, sup_control_rx),
        RunConfig::<TokioRuntime>::root(),
        sup_id,
    ));

    // ── 6. Send work ────────────────────────────────────────────────────────
    //
    // Back-to-back: the first SpawnWorker transitions Idle → Spawning, the
    // second is buffered in the pool's spawn queue and processed once the
    // first worker's SpawnedWorker reply arrives.
    pool_ref
        .send(pool_id, PoolMsg::SpawnWorker(SpawnWorker { task_id: 0 }))
        .await
        .expect("pool channel open");
    pool_ref
        .send(pool_id, PoolMsg::SpawnWorker(SpawnWorker { task_id: 1 }))
        .await
        .expect("pool channel open");

    // ── 7. Wait for group shutdown ──────────────────────────────────────────
    //
    // WhenAllDone fires once the pool and both workers have reported Stopped;
    // the supervisor then returns Decision::Done and its root run loop exits.
    // The timeout is only a deadlock backstop, not synchronization.
    let events_summary = || {
        seen.lock()
            .map(|e| format!("{:?}", e))
            .unwrap_or_else(|_| "<lock poisoned>".to_string())
    };
    tokio::time::timeout(Duration::from_secs(10), sup_task)
        .await
        .unwrap_or_else(|_| {
            panic!(
                "supervisor did not shut down within 10s; events seen: {}",
                events_summary()
            )
        })
        .expect("supervisor task panicked");

    // ── 8. Assertions on the recorded lifecycle event stream ────────────────
    //
    // Assertion shapes are chosen for determinism:
    //
    // - `Started` counts are exact: each child leaves Init exactly once (no
    //   Reset in this flow).
    // - `Stopped` is asserted per distinct child id: after the last genuine
    //   stop, the supervisor enters ShuttingDown and `stop_all_children`
    //   sends Stop to every child; children already suspended in Init
    //   acknowledge with a duplicate `Stopped`. How many of those acks land
    //   before this point is racy, so raw counts would be flaky — the set of
    //   stopped ids is not (the supervisor could only shut down after the
    //   genuine Stopped of every child).
    let events = seen.lock().expect("collector lock poisoned");

    let pool_started_count = events
        .iter()
        .filter(|e| matches!(e, ChildLifecycleEvent::Started { child_id } if *child_id == pool_id))
        .count();
    let worker_started_count = events
        .iter()
        .filter(|e| matches!(e, ChildLifecycleEvent::Started { child_id } if *child_id != pool_id))
        .count();
    let stopped_worker_ids: std::collections::BTreeSet<usize> = events
        .iter()
        .filter_map(|e| match e {
            ChildLifecycleEvent::Stopped { child_id } if *child_id != pool_id => Some(*child_id),
            _ => None,
        })
        .collect();
    let pool_stopped_count = events
        .iter()
        .filter(|e| matches!(e, ChildLifecycleEvent::Stopped { child_id } if *child_id == pool_id))
        .count();
    let failed_count = events
        .iter()
        .filter(|e| matches!(e, ChildLifecycleEvent::Failed { .. }))
        .count();

    assert_eq!(
        pool_started_count, 1,
        "expected exactly 1 pool Started event: events={:?}",
        events
    );
    assert_eq!(
        worker_started_count, 2,
        "expected 2 worker Started events: events={:?}",
        events
    );
    assert_eq!(
        stopped_worker_ids.len(),
        2,
        "expected both workers to report Stopped (each self-stops via Decision::Stop \
         after its DoWork): events={:?}",
        events
    );
    assert!(
        pool_stopped_count >= 1,
        "expected the pool to report Stopped once its pending count drained to 0: events={:?}",
        events
    );
    assert_eq!(
        failed_count, 0,
        "expected no Failed events: events={:?}",
        events
    );

    drop(events);
    collector_task.abort();
}
