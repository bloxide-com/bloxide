// Copyright 2025 Bloxide, all rights reserved
//! Integration test for the full pool-demo supervision lifecycle.
//!
//! This test exercises the real Tokio runtime end-to-end:
//!
//! 1. Creates a supervisor with a `ChildGroupBuilder`.
//! 2. Creates pool channels (`PoolMsg` + `SpawnedWorker` reply).
//! 3. Builds a `PoolCtx` wired to the supervisor's control/notify channels.
//! 4. Spawns the pool as a supervised child task (via `run_supervised_actor`).
//! 5. Sends `SpawnWorker` messages to the pool.
//! 6. The pool calls `spawn_worker`, which creates worker tokio tasks and
//!    registers them with the supervisor via `RegisterDynamicChild`.
//! 7. The pool sends `DoWork` to each worker; the worker transitions to
//!    `Done` (terminal), sends `WorkDone` to the pool, and the runtime
//!    reports `ChildLifecycleEvent::Done` to the supervisor.
//! 8. The test drives the supervisor manually (dispatching both notify and
//!    control events) and verifies the event sequence:
//!    `Started` (pool), `Started` (worker), `Done` (worker), …, `Done` (pool).

use core::future::poll_fn;
use core::pin::Pin;

use bloxide_child_management::{ChildPolicy, GroupShutdown};
use bloxide_core::lifecycle::{ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::Envelope;
use bloxide_core::{capability::DynamicChannelCap, StateMachine};
use bloxide_supervisor::{SupervisorControl, SupervisorCtx, SupervisorEvent, SupervisorSpec};
use bloxide_tokio::{run_supervised_actor, ChildGroupBuilder, TokioRuntime, TokioStream};
use futures_core::Stream;
use pool_blox::{PoolCtx, PoolSpec};
use pool_messages::{PoolMsg, SpawnWorker};
use tokio_pool_demo_impl::spawn_worker;

/// Receive the next envelope from a TokioStream, or None if closed.
async fn recv<M: Send + 'static>(rx: &mut TokioStream<M>) -> Option<Envelope<M>> {
    poll_fn(|cx| Pin::new(&mut *rx).poll_next(cx)).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pool_lifecycle_spawn_and_done() {
    let timeout = std::time::Duration::from_secs(10);

    // ── 1. Create pool channels ────────────────────────────────────────────
    //
    // The pool has two domain mailboxes:
    //   - PoolMsg (incoming SpawnWorker / WorkDone)
    //   - SpawnedWorker<R> (reply from the spawn factory)
    let pool_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
    let (pool_ref, pool_msg_rx) =
        <TokioRuntime as DynamicChannelCap>::channel::<PoolMsg>(pool_id, 32);
    let (spawn_reply_ref, spawn_reply_rx) =
        <TokioRuntime as DynamicChannelCap>::channel::<pool_messages::SpawnedWorker<TokioRuntime>>(
            pool_id, 32,
        );

    // ── 2. Create the supervisor's child group builder ─────────────────────
    //
    // The builder allocates the notify channel (children → supervisor) and
    // the control channel (pool → supervisor for RegisterDynamicChild).
    let mut group_builder: ChildGroupBuilder<TokioRuntime, SupervisorControl<TokioRuntime>> =
        ChildGroupBuilder::new(GroupShutdown::WhenAllDone);
    let sup_control_ref = group_builder.control_ref();
    let sup_notify_ref = group_builder.notify_ref();

    // ── 3. Build the PoolCtx and state machine ─────────────────────────────
    let pool_ctx = PoolCtx::new(
        pool_id,
        pool_ref.clone(),
        spawn_worker as _,
        sup_control_ref.clone(),
        sup_notify_ref.clone(),
        spawn_reply_ref.clone(),
    );
    let pool_machine = StateMachine::<PoolSpec<TokioRuntime>>::new(pool_ctx);

    // ── 4. Spawn the pool as a supervised child ────────────────────────────
    //
    // add_child creates the pool's lifecycle channel and returns
    // (lifecycle_rx, supervisor_notify_sender).
    let (pool_lifecycle_rx, pool_sup_notify) =
        group_builder.add_child(pool_id, ChildPolicy::Stop);

    // The pool's domain mailboxes are (PoolMsg stream, SpawnedWorker stream).
    let pool_domain_mailboxes = (pool_msg_rx, spawn_reply_rx);

    // Spawn the pool actor task using the supervised runner.
    tokio::spawn(run_supervised_actor::<PoolSpec<TokioRuntime>>(
        pool_machine,
        pool_domain_mailboxes,
        pool_lifecycle_rx,
        pool_id,
        pool_sup_notify,
    ));

    // ── 5. Build the supervisor state machine ──────────────────────────────
    let sup_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
    let (children, mut sup_notify_rx, mut sup_control_rx) = group_builder.finish();
    let sup_ctx = SupervisorCtx::new(sup_id, children, sup_notify_ref);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);

    // Start the supervisor — it sends Start to all registered children (the pool).
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));

    // ── 6. Send SpawnWorker messages to the pool ───────────────────────────
    let _ = pool_ref
        .send(pool_id, PoolMsg::SpawnWorker(SpawnWorker { task_id: 0 }))
        .await;
    let _ = pool_ref
        .send(pool_id, PoolMsg::SpawnWorker(SpawnWorker { task_id: 1 }))
        .await;

    // ── 7. Drive the supervisor and collect lifecycle events ───────────────
    //
    // The supervisor has two input channels:
    //   - sup_notify_rx: ChildLifecycleEvent from children
    //   - sup_control_rx: SupervisorControl (RegisterDynamicChild from spawn)
    //
    // We poll both and dispatch events to the supervisor machine, collecting
    // ChildLifecycleEvents for assertions.
    //
    // Expected event sequence:
    //   1. Started { pool_id }     — pool enters Idle after Start
    //   2. RegisterDynamicChild    — pool spawns worker, registers it
    //   3. Started { worker_1 }    — worker enters Waiting after Start
    //   4. Done { worker_1 }       — worker processes DoWork → Done (terminal)
    //   5. RegisterDynamicChild    — pool spawns second worker
    //   6. Started { worker_2 }    — worker enters Waiting
    //   7. Done { worker_2 }       — worker processes DoWork → Done
    //   8. Done { pool_id }        — pool reaches AllDone after all WorkDone
    //
    // The pool uses a spawn queue: sending two SpawnWorker messages back-to-back
    // means the first transitions Idle→Spawning, the second is queued. After the
    // first worker is spawned and replies, the pool processes the queue and
    // spawns the second worker.

    let mut events: Vec<ChildLifecycleEvent> = Vec::new();
    let mut pool_done = false;

    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    loop {
        if pool_done {
            break;
        }
        tokio::select! {
            biased;
            _ = &mut deadline => {
                panic!(
                    "timeout: events seen so far: {:?}",
                    events
                );
            }
            // Check control channel (RegisterDynamicChild from spawn_child)
            envelope = recv(&mut sup_control_rx) => {
                if let Some(env) = envelope {
                    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Control(env));
                }
            }
            // Check notify channel (ChildLifecycleEvent from children)
            envelope = recv(&mut sup_notify_rx) => {
                if let Some(env) = envelope {
                    // ChildLifecycleEvent is Copy — clone before dispatch.
                    let event = env.1;
                    if let ChildLifecycleEvent::Done { child_id } = &event {
                        if *child_id == pool_id {
                            pool_done = true;
                        }
                    }
                    events.push(event);
                    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Child(
                        Envelope(0, event),
                    ));
                }
            }
        }
    }

    // ── 8. Assertions ──────────────────────────────────────────────────────

    // We should see at least:
    //   - 1 pool Started
    //   - 2 worker Started
    //   - 2 worker Done
    //   - 1 pool Done
    let pool_started = events
        .iter()
        .any(|e| matches!(e, ChildLifecycleEvent::Started { child_id } if *child_id == pool_id));
    let worker_started_count = events
        .iter()
        .filter(|e| matches!(e, ChildLifecycleEvent::Started { child_id } if *child_id != pool_id))
        .count();
    let worker_done_count = events
        .iter()
        .filter(|e| matches!(e, ChildLifecycleEvent::Done { child_id } if *child_id != pool_id))
        .count();
    let pool_done_count = events
        .iter()
        .filter(|e| matches!(e, ChildLifecycleEvent::Done { child_id } if *child_id == pool_id))
        .count();

    assert!(
        pool_started,
        "expected pool Started event: events={:?}",
        events
    );
    assert_eq!(
        worker_started_count, 2,
        "expected 2 worker Started events: events={:?}",
        events
    );
    assert_eq!(
        worker_done_count, 2,
        "expected 2 worker Done events: events={:?}",
        events
    );
    assert_eq!(
        pool_done_count, 1,
        "expected 1 pool Done event: events={:?}",
        events
    );
}
