// Copyright 2025 Bloxide, all rights reserved
//! Pool action functions.
use blox_ctx_workers::HasWorkers;
use bloxide_core::{capability::BloxRuntime, transition::ActionResult, HasSelfId};
use pool_messages::PoolMsg;

use crate::{PoolCtx, PoolEvent};

// Dynamic-only imports and action functions.
#[cfg(feature = "dynamic")]
use bloxide_peers::introduce_peers;
#[cfg(feature = "dynamic")]
use bloxide_spawn::spawn_child;
#[cfg(feature = "dynamic")]
use bloxide_supervisor::SupervisorRegistrar;
#[cfg(feature = "dynamic")]
use pool_messages::{DoWork, SpawnRequest, SpawnWorker, WorkerMsg};

/// Handle a SpawnWorker request: call the spawn helper to create a child,
/// then transition to the Spawning state to wait for the reply.
#[cfg(feature = "dynamic")]
pub fn handle_spawn_worker<R: BloxRuntime>(
    ctx: &mut PoolCtx<R>,
    ev: &PoolEvent<R>,
) -> ActionResult {
    if let Some(PoolMsg::SpawnWorker(SpawnWorker { task_id })) = ev.msg_payload() {
        bloxide_log::blox_log_info!(ctx.self_id(), "spawning worker for task_id={}", task_id);
        ctx.pending_task_id = *task_id;
        ctx.spawn_in_flight = true;
        let req = SpawnRequest::Worker {
            task_id: *task_id,
            reply_to: ctx.spawn_reply_ref.clone(),
            pool_ref: ctx.self_ref.clone(),
        };
        let result = spawn_child::<_, _, SupervisorRegistrar>(
            ctx.spawn_fn,
            req,
            &ctx.spawn_ref,
            &ctx.notify_ref,
            ctx.self_id(),
        );
        if result.is_err() {
            bloxide_log::blox_log_warn!(
                ctx.self_id(),
                "spawn failed (supervisor control mailbox full), dropping task_id={}",
                task_id
            );
            ctx.spawn_in_flight = false;
        }
    }
    ActionResult::Ok
}

/// Buffer a SpawnWorker request while already in Spawning state.
/// The task_id is queued and will be processed after the current spawn reply arrives.
#[cfg(feature = "dynamic")]
pub fn handle_spawn_worker_queued<R: BloxRuntime>(
    ctx: &mut PoolCtx<R>,
    ev: &PoolEvent<R>,
) -> ActionResult {
    if let Some(PoolMsg::SpawnWorker(SpawnWorker { task_id })) = ev.msg_payload() {
        bloxide_log::blox_log_debug!(
            ctx.self_id(),
            "queuing spawn request for task_id={} (already spawning)",
            task_id
        );
        ctx.spawn_queue.push(*task_id);
    }
    ActionResult::Ok
}

/// Handle a SpawnedWorker reply: store the worker refs, introduce peers,
/// send DoWork, and transition to Active (or back to Spawning if queue is non-empty).
#[cfg(feature = "dynamic")]
pub fn handle_spawned_worker<R: BloxRuntime>(
    ctx: &mut PoolCtx<R>,
    ev: &PoolEvent<R>,
) -> ActionResult {
    if let Some(spawned) = ev.spawn_reply_payload() {
        // Spawn reply received — clear the in-flight flag.
        ctx.spawn_in_flight = false;

        let task_id = ctx.pending_task_id;
        bloxide_log::blox_log_info!(
            ctx.self_id(),
            "worker spawned: child_id={} task_id={}",
            spawned.child_id,
            task_id
        );
        let domain_ref = spawned.domain_ref.clone();
        let ctrl_ref = spawned.ctrl_ref.clone();
        ctx.worker_refs_mut().push(domain_ref.clone());
        ctx.worker_ctrls_mut().push(ctrl_ref);
        ctx.set_pending(ctx.pending() + 1);
        // Introduce the newest worker to all existing workers (bidirectional).
        {
            let n = ctx.worker_refs().len();
            if n >= 2 {
                let new_idx = n - 1;
                let from = ctx.self_id();
                let new_id = ctx.worker_refs()[new_idx].id();
                let new_ref = ctx.worker_refs()[new_idx].clone();
                let new_ctrl = ctx.worker_ctrls()[new_idx].clone();
                for i in 0..new_idx {
                    let old_id = ctx.worker_refs()[i].id();
                    let old_ref = ctx.worker_refs()[i].clone();
                    let old_ctrl = ctx.worker_ctrls()[i].clone();
                    introduce_peers(
                        from,
                        new_id,
                        new_ref.clone(),
                        new_ctrl.clone(),
                        old_id,
                        old_ref.clone(),
                        old_ctrl.clone(),
                    );
                }
            }
        }
        let self_id = ctx.self_id();
        if domain_ref
            .try_send(self_id, WorkerMsg::DoWork(DoWork { task_id }))
            .is_err()
        {
            bloxide_log::blox_log_warn!(
                self_id,
                "worker channel full, dropping task_id={}",
                task_id
            );
            if ctx.pending() > 0 {
                ctx.set_pending(ctx.pending() - 1);
            }
        }

        // If there are queued spawn requests, start the next one immediately (FIFO).
        if !ctx.spawn_queue.is_empty() {
            let next_task_id = ctx.spawn_queue.remove(0);
            bloxide_log::blox_log_info!(
                ctx.self_id(),
                "processing queued spawn for task_id={}",
                next_task_id
            );
            ctx.pending_task_id = next_task_id;
            ctx.spawn_in_flight = true;
            let req = SpawnRequest::Worker {
                task_id: next_task_id,
                reply_to: ctx.spawn_reply_ref.clone(),
                pool_ref: ctx.self_ref.clone(),
            };
            let result = spawn_child::<_, _, SupervisorRegistrar>(
                ctx.spawn_fn,
                req,
                &ctx.spawn_ref,
                &ctx.notify_ref,
                ctx.self_id(),
            );
            if result.is_err() {
                bloxide_log::blox_log_warn!(
                    ctx.self_id(),
                    "spawn failed (supervisor control mailbox full), dropping queued task_id={}",
                    next_task_id
                );
                ctx.spawn_in_flight = false;
            }
        }
    }
    ActionResult::Ok
}

pub fn handle_work_done<R: BloxRuntime>(ctx: &mut PoolCtx<R>, ev: &PoolEvent<R>) -> ActionResult {
    if let Some(PoolMsg::WorkDone(done)) = ev.msg_payload() {
        bloxide_log::blox_log_info!(
            ctx.self_id(),
            "worker {} done: task_id={} result={}",
            done.worker_id,
            done.task_id,
            done.result
        );
        if ctx.pending() > 0 {
            ctx.set_pending(ctx.pending() - 1);
        }
    }
    ActionResult::Ok
}

pub fn log_all_done<R: BloxRuntime>(ctx: &mut PoolCtx<R>) {
    bloxide_log::blox_log_info!(
        ctx.self_id(),
        "all {} workers done",
        ctx.worker_refs().len()
    );
}
