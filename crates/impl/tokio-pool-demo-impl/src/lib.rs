// Copyright 2025 Bloxide, all rights reserved
//! Concrete runtime-specific worker spawn function for the Tokio pool demo.
//!
//! This crate is the impl layer for `tokio-pool-demo`. It is the only place
//! that knows about concrete worker context/spec types and task spawning.

extern crate alloc;

use bloxide_child_management::ChildPolicy;
use bloxide_core::lifecycle::AbortCommand;
use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::ActorRef,
    StateMachine,
};
use bloxide_peers::PeerCtrl;
use bloxide_spawn::{SpawnCap, SpawnOutput};
use bloxide_tokio::{run, RunConfig, TokioRuntime};
use pool_messages::{DoWork, SpawnRequest, SpawnedWorker, WorkerMsg};
use worker_blox::{WorkerCtx, WorkerSpec};

/// Process a work request: store the task ID and compute the result (task_id * 2).
pub fn process_work(task_id: &mut u32, result: &mut u32, do_work: &DoWork) {
    *task_id = do_work.task_id;
    *result = do_work.task_id * 2;
}

// ── Pool action functions (Phase 3 system codegen) ──────────────────────────
// These are called by the generated concrete spec_skeleton with individual
// context fields passed as parameters (not the full PoolCtx).

/// Decrement the pending work counter when a WorkDone is received.
pub fn handle_work_done(pending: &mut u32, _work_done: &pool_messages::WorkDone) {
    if *pending > 0 {
        *pending -= 1;
    }
}

/// Spawn a new worker via the supervisor, then set in-flight flag.
#[cfg(feature = "dynamic")]
pub fn handle_spawn_worker<R: BloxRuntime>(
    self_id: bloxide_core::ActorId,
    spawn_fn: &bloxide_spawn::SpawnFn<
        R,
        pool_messages::SpawnRequest<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
    >,
    spawn_ref: &bloxide_core::messaging::ActorRef<bloxide_supervisor::SupervisorControl<R>, R>,
    pending_task_id: &mut u32,
    spawn_in_flight: &mut bool,
    _spawn_queue: &mut alloc::vec::Vec<u32>,
    _worker_refs: &mut alloc::vec::Vec<bloxide_core::messaging::ActorRef<WorkerMsg, R>>,
    _worker_ctrls: &mut alloc::vec::Vec<
        bloxide_core::messaging::ActorRef<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
    >,
    pending: &mut u32,
    spawn_worker: &pool_messages::SpawnWorker,
) {
    let task_id = spawn_worker.task_id;
    *pending_task_id = task_id;
    *spawn_in_flight = true;

    // We need the reply and pool refs — but these aren't in the field list.
    // The spawn_child helper needs them. For now, we can't call spawn_child
    // because we don't have spawn_reply_ref or self_ref.
    // TODO: Add spawn_reply_ref and self_ref to the action's field list.
    // For now, just increment pending to track the in-flight work.
    *pending += 1;
}

/// Buffer a SpawnWorker request while already in Spawning state.
#[cfg(feature = "dynamic")]
pub fn handle_spawn_worker_queued(
    spawn_queue: &mut alloc::vec::Vec<u32>,
    spawn_worker: &pool_messages::SpawnWorker,
) {
    spawn_queue.push(spawn_worker.task_id);
}

/// Handle a SpawnedWorker reply: store worker refs, send DoWork, process queue.
#[cfg(feature = "dynamic")]
pub fn handle_spawned_worker<R: BloxRuntime>(
    spawn_in_flight: &mut bool,
    spawn_queue: &mut alloc::vec::Vec<u32>,
    _worker_refs: &mut alloc::vec::Vec<bloxide_core::messaging::ActorRef<WorkerMsg, R>>,
    _worker_ctrls: &mut alloc::vec::Vec<
        bloxide_core::messaging::ActorRef<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
    >,
    _pending: &mut u32,
    _spawned_worker: &pool_messages::SpawnedWorker<
        bloxide_peers::PeerCtrl<WorkerMsg, R>,
        R,
    >,
) {
    *spawn_in_flight = false;
    // TODO: Full implementation needs self_ref, spawn_fn, spawn_ref, notify_ref,
    // spawn_reply_ref, and self_id to introduce peers and send DoWork.
    // For now, just clear in-flight and drain the queue.
    if !spawn_queue.is_empty() {
        spawn_queue.remove(0);
        *spawn_in_flight = true;
    }
}

/// Spawn function for the Tokio pool demo.
///
/// Creates a worker actor and returns the handles the supervisor needs.
/// This is a plain function (not a trait impl) — the wiring layer passes
/// it to `spawn_child()` as a `SpawnFn<R, SpawnRequest<R>>`.
///
/// All state comes from the request — `pool_ref` is in the message, not
/// captured from a struct field.
pub fn spawn_worker(
    req: SpawnRequest<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>,
    notify: ActorRef<ChildLifecycleEvent, TokioRuntime>,
) -> SpawnOutput<TokioRuntime> {
    match req {
        SpawnRequest::Worker {
            task_id: _,
            reply_to,
            pool_ref,
        } => {
            let worker_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
            let (ctrl_ref, ctrl_rx) = <TokioRuntime as DynamicChannelCap>::channel::<
                PeerCtrl<WorkerMsg, TokioRuntime>,
            >(worker_id, 16);
            let (domain_ref, domain_rx) =
                <TokioRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 16);
            let (lifecycle_ref, lifecycle_rx) =
                <TokioRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(worker_id, 4);
            let (abort_ref, abort_rx) =
                <TokioRuntime as DynamicChannelCap>::channel::<AbortCommand>(worker_id, 4);

            let worker_ctx = WorkerCtx::new(worker_id, pool_ref);
            let machine = StateMachine::<WorkerSpec<TokioRuntime>>::new(worker_ctx);

            let notify_sender = notify.sender();
            let task_handle = <TokioRuntime as SpawnCap>::spawn(async move {
                run(
                    machine,
                    (ctrl_rx, domain_rx),
                    RunConfig::<TokioRuntime>::supervised_with_abort(
                        lifecycle_rx,
                        abort_rx,
                        notify_sender,
                    ),
                    worker_id,
                )
                .await
            });

            // Convert the JoinHandle (not Clone) into a KillHandle (Clone)
            // so it can be stored in RegisterDynamicChild and cloned from
            // &Event by the supervisor's action function.
            let kill_handle = <TokioRuntime as SpawnCap>::kill_handle(task_handle);

            let _ = reply_to.try_send(
                worker_id,
                SpawnedWorker {
                    child_id: worker_id,
                    domain_ref: domain_ref.clone(),
                    ctrl_ref: ctrl_ref.clone(),
                },
            );

            SpawnOutput {
                child_id: worker_id,
                lifecycle_ref,
                abort_ref,
                kill_handle,
                policy: ChildPolicy::Stop,
            }
        }
    }
}
