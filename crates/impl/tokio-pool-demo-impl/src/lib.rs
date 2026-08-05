// Copyright 2025 Bloxide, all rights reserved
//! Concrete runtime-specific worker factory for the Tokio pool demo.
//!
//! This crate is the impl layer for `tokio-pool-demo`. It does domain
//! construction (channels, worker context, state machine); executor
//! mechanics (`RunConfig`, task spawn, kill handle) live in bloxide-spawn.

extern crate alloc;

use blox_ctx_pool_ref::{SpawnRequest, SpawnedWorker};
use bloxide_child_management::ChildPolicy;
#[cfg(feature = "dynamic")]
use bloxide_core::capability::BloxRuntime;
use bloxide_core::lifecycle::AbortCommand;
#[cfg(feature = "dynamic")]
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::{
    capability::DynamicChannelCap, lifecycle::LifecycleCommand, transition::ActionResult,
    StateMachine,
};
use bloxide_peers::PeerCtrl;
use bloxide_spawn::ActorParts;
use bloxide_tokio::TokioRuntime;
use pool_messages::{DoWork, WorkerMsg};
use worker_blox::WorkerCtx;

/// Process a work request: store the task ID and compute the result (task_id * 2).
pub fn process_work(task_id: &mut u32, result: &mut u32, do_work: &DoWork) -> ActionResult {
    *task_id = do_work.task_id;
    *result = do_work.task_id * 2;
    println!(
        "[worker] task {} processed -> result = {}",
        *task_id, *result
    );
    ActionResult::Ok
}

// ── Pool action functions (Phase 3 system codegen) ──────────────────────────
// These are called by the generated concrete spec_skeleton with individual
// context fields passed as parameters (not the full PoolCtx).

/// Decrement the pending work counter when a WorkDone is received.
pub fn handle_work_done(pending: &mut u32, work_done: &pool_messages::WorkDone) -> ActionResult {
    if *pending > 0 {
        *pending -= 1;
    }
    println!(
        "[pool] task {} done by worker {} (result = {}) — {} still pending",
        work_done.task_id, work_done.worker_id, work_done.result, *pending
    );
    ActionResult::Ok
}

/// Spawn a new worker via the supervisor, then set in-flight flag.
// Action functions take concrete params by design (the codegen wrapper passes
// individual context fields), so the argument count exceeds clippy's default.
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "dynamic")]
pub fn handle_spawn_worker<R: BloxRuntime>(
    self_id: bloxide_core::ActorId,
    self_ref: &bloxide_core::messaging::ActorRef<pool_messages::PoolMsg, R>,
    spawn_fn: &bloxide_spawn::SpawnFn<
        R,
        blox_ctx_pool_ref::SpawnRequest<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
    >,
    spawn_ref: &bloxide_core::messaging::ActorRef<
        bloxide_child_management::control::ChildCtrl<R>,
        R,
    >,
    notify_ref: &bloxide_core::messaging::ActorRef<ChildLifecycleEvent, R>,
    spawn_reply_ref: &bloxide_core::messaging::ActorRef<
        blox_ctx_pool_ref::SpawnedWorker<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
        R,
    >,
    pending_task_id: &mut u32,
    spawn_in_flight: &mut bool,
    pending: &mut u32,
    spawn_worker: &pool_messages::SpawnWorker,
) -> ActionResult {
    let task_id = spawn_worker.task_id;
    *pending_task_id = task_id;
    *spawn_in_flight = true;
    *pending += 1;
    println!(
        "[pool] spawn requested for task {} (pending = {})",
        task_id, *pending
    );

    let req = blox_ctx_pool_ref::SpawnRequest::Worker {
        task_id,
        reply_to: spawn_reply_ref.clone(),
        pool_ref: self_ref.clone(),
    };
    ActionResult::from(bloxide_spawn::spawn_dynamic_child::<
        _,
        _,
        bloxide_spawn::ChildCtrlRegistrar,
    >(*spawn_fn, req, spawn_ref, notify_ref, self_id))
}

/// Buffer a SpawnWorker request while already in Spawning state.
#[cfg(feature = "dynamic")]
pub fn handle_spawn_worker_queued(
    spawn_queue: &mut alloc::vec::Vec<u32>,
    pending: &mut u32,
    spawn_worker: &pool_messages::SpawnWorker,
) -> ActionResult {
    spawn_queue.push(spawn_worker.task_id);
    *pending += 1;
    println!(
        "[pool] spawn for task {} queued behind in-flight spawn (pending = {})",
        spawn_worker.task_id, *pending
    );
    ActionResult::Ok
}

/// Handle a SpawnedWorker reply: store worker refs, introduce peers,
/// send DoWork, and process the next queued spawn if any.
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "dynamic")]
pub fn handle_spawned_worker<R: BloxRuntime>(
    self_id: bloxide_core::ActorId,
    self_ref: &bloxide_core::messaging::ActorRef<pool_messages::PoolMsg, R>,
    spawn_fn: &bloxide_spawn::SpawnFn<
        R,
        blox_ctx_pool_ref::SpawnRequest<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
    >,
    spawn_ref: &bloxide_core::messaging::ActorRef<
        bloxide_child_management::control::ChildCtrl<R>,
        R,
    >,
    notify_ref: &bloxide_core::messaging::ActorRef<ChildLifecycleEvent, R>,
    spawn_reply_ref: &bloxide_core::messaging::ActorRef<
        blox_ctx_pool_ref::SpawnedWorker<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
        R,
    >,
    pending_task_id: &mut u32,
    spawn_in_flight: &mut bool,
    spawn_queue: &mut alloc::vec::Vec<u32>,
    worker_refs: &mut alloc::vec::Vec<bloxide_core::messaging::ActorRef<WorkerMsg, R>>,
    worker_ctrls: &mut alloc::vec::Vec<
        bloxide_core::messaging::ActorRef<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
    >,
    spawned_worker: &blox_ctx_pool_ref::SpawnedWorker<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
) -> ActionResult {
    *spawn_in_flight = false;

    let new_worker_id = spawned_worker.child_id;
    let new_domain_ref = spawned_worker.domain_ref.clone();
    let new_ctrl_ref = spawned_worker.ctrl_ref.clone();

    // Introduce the new worker to all existing workers (bidirectional).
    for i in 0..worker_refs.len() {
        bloxide_peers::introduce_peers(
            self_id,
            worker_refs[i].id(),
            worker_refs[i].clone(),
            worker_ctrls[i].clone(),
            new_worker_id,
            new_domain_ref.clone(),
            new_ctrl_ref.clone(),
        );
    }

    // Send DoWork to the new worker.
    let mut result = ActionResult::from(new_domain_ref.try_send(
        self_id,
        WorkerMsg::DoWork(DoWork {
            task_id: *pending_task_id,
        }),
    ));

    // Store the new worker's refs.
    worker_refs.push(new_domain_ref);
    worker_ctrls.push(new_ctrl_ref);
    println!(
        "[pool] worker {} up — DoWork task {} dispatched ({} peer(s) introduced)",
        new_worker_id,
        *pending_task_id,
        worker_refs.len() - 1
    );

    // Process the next queued spawn if any.
    if let Some(&next_task_id) = spawn_queue.first() {
        spawn_queue.remove(0);
        *pending_task_id = next_task_id;
        *spawn_in_flight = true;

        let req = blox_ctx_pool_ref::SpawnRequest::Worker {
            task_id: next_task_id,
            reply_to: spawn_reply_ref.clone(),
            pool_ref: self_ref.clone(),
        };
        let r = bloxide_spawn::spawn_dynamic_child::<_, _, bloxide_spawn::ChildCtrlRegistrar>(
            *spawn_fn, req, spawn_ref, notify_ref, self_id,
        );
        if r.is_err() {
            result = ActionResult::from(r);
        }
    }
    result
}

/// Build the worker's [`ActorParts`] for the Tokio pool demo.
///
/// Creates everything the platform spawn needs — actor id, channels, worker
/// context, and state machine — and returns the parts. The task spawn itself
/// is performed by `bloxide_spawn::spawn_actor_task`, which the wiring layer
/// composes with this function into a `SpawnFn<R, SpawnRequest<R>>`.
///
/// Generic over the worker spec type `S` so the system-level codegen can
/// inject the concrete `WorkerSpec` (with real action closures) instead of
/// the blox-crate-level stub spec. The generated main.rs monomorphizes this
/// function with the system-level spec via a wrapper.
///
/// All state comes from the request — `pool_ref` is in the message, not
/// captured from a struct field.
///
/// The `Mailboxes<TokioRuntime>` equality binding pins the spec's mailbox
/// GAT to the concrete channel tuple this factory creates (`ActorParts`
/// types the field as `S::Mailboxes<R>`, unlike `run()` which was generic
/// over the tuple).
pub fn build_worker<S>(
    req: SpawnRequest<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>,
) -> ActorParts<S, TokioRuntime>
where
    S: bloxide_core::spec::MachineSpec<
        Ctx = WorkerCtx<TokioRuntime>,
        Mailboxes<TokioRuntime> = (
            bloxide_tokio::TokioStream<PeerCtrl<WorkerMsg, TokioRuntime>>,
            bloxide_tokio::TokioStream<WorkerMsg>,
        ),
    >,
    S::Event: From<bloxide_core::messaging::Envelope<PeerCtrl<WorkerMsg, TokioRuntime>>>
        + From<bloxide_core::messaging::Envelope<WorkerMsg>>,
{
    match req {
        SpawnRequest::Worker {
            task_id,
            reply_to,
            pool_ref,
        } => {
            let worker_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
            println!("[spawn] worker {} created for task {}", worker_id, task_id);
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
            let machine = StateMachine::<S>::new(worker_ctx);

            // The reply is sent BEFORE the task is spawned (the platform
            // helper spawns it only after this function returns). This is
            // safe: the pool processes `SpawnedWorker` in a later dispatch
            // (run-to-completion), after the synchronous spawn composition
            // completes.
            let _ = reply_to.try_send(
                worker_id,
                SpawnedWorker {
                    child_id: worker_id,
                    domain_ref: domain_ref.clone(),
                    ctrl_ref: ctrl_ref.clone(),
                },
            );

            ActorParts {
                child_id: worker_id,
                machine,
                mailboxes: (ctrl_rx, domain_rx),
                lifecycle_ref,
                lifecycle_rx,
                abort_ref,
                abort_rx,
                policy: ChildPolicy::Stop,
            }
        }
    }
}
