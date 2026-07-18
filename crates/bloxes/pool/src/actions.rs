// Copyright 2025 Bloxide, all rights reserved
//! Pool action functions and state handler tables.
use blox_ctx_workers::HasWorkers;
use bloxide_core::{
    capability::BloxRuntime, spec::StateFns, transition::ActionResult, transitions, HasSelfId,
};
use pool_actions::actions::introduce_new_worker;
use pool_messages::{AppSpawnRequest, DoWork, PoolMsg, SpawnWorker, WorkerMsg};

pub use crate::generated::topology::PoolState;
use crate::{PoolCtx, PoolEvent, PoolSpec};

/// Handle a SpawnWorker request: send an AppSpawnRequest to the supervisor
/// and transition to the Spawning state.
pub fn handle_spawn_worker<R: BloxRuntime>(
    ctx: &mut PoolCtx<R>,
    ev: &PoolEvent<R>,
) -> ActionResult {
    if let Some(PoolMsg::SpawnWorker(SpawnWorker { task_id })) = ev.msg_payload() {
        bloxide_log::blox_log_info!(ctx.self_id(), "spawning worker for task_id={}", task_id);
        ctx.pending_task_id = *task_id;
        ctx.spawn_in_flight = true;
        let req = AppSpawnRequest::Worker {
            task_id: *task_id,
            reply_to: ctx.spawn_reply_ref.clone(),
        };
        let self_id = ctx.self_id();
        if ctx.spawn_ref.try_send(self_id, req).is_err() {
            bloxide_log::blox_log_warn!(
                self_id,
                "spawn mailbox full, dropping task_id={}",
                task_id
            );
            ctx.spawn_in_flight = false;
        }
    }
    ActionResult::Ok
}

/// Buffer a SpawnWorker request while already in Spawning state.
/// The task_id is queued and will be processed after the current spawn reply arrives.
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
        introduce_new_worker(ctx);
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
            let req = AppSpawnRequest::Worker {
                task_id: next_task_id,
                reply_to: ctx.spawn_reply_ref.clone(),
            };
            let self_id = ctx.self_id();
            if ctx.spawn_ref.try_send(self_id, req).is_err() {
                bloxide_log::blox_log_warn!(
                    self_id,
                    "spawn mailbox full, dropping queued task_id={}",
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

impl<R> PoolSpec<R>
where
    R: BloxRuntime,
{
    pub(crate) const IDLE_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PoolMsg::SpawnWorker(_) => {
                actions [handle_spawn_worker]
                transition PoolState::Spawning
            },
        ],
    };

    pub(crate) const SPAWNING_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PoolEvent::<R>::SpawnReply(_) => {
                actions [handle_spawned_worker]
                guard(ctx, _results) {
                    // If we just kicked off another spawn from the queue, stay in Spawning
                    ctx.spawn_in_flight => PoolState::Spawning,
                    // If there are still queued spawns (shouldn't happen without in-flight), keep spawning
                    !ctx.spawn_queue.is_empty() => PoolState::Spawning,
                    // If all workers already finished while we were spawning, done
                    ctx.pending() == 0 && ctx.worker_refs().len() > 0 => PoolState::AllDone,
                    // Otherwise go active
                    _ => PoolState::Active,
                }
            },
            PoolMsg::SpawnWorker(_) => {
                actions [handle_spawn_worker_queued]
                stay
            },
            PoolMsg::WorkDone(_) => {
                actions [handle_work_done]
                stay
            },
        ],
    };

    pub(crate) const ACTIVE_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PoolMsg::SpawnWorker(_) => {
                actions [handle_spawn_worker]
                transition PoolState::Spawning
            },
            PoolMsg::WorkDone(_done) => {
                actions [handle_work_done]
                guard(ctx, _results) {
                    // Don't go to AllDone if a spawn is still in-flight
                    ctx.pending() == 0 && !ctx.spawn_in_flight => PoolState::AllDone,
                    _ => stay,
                }
            },
        ],
    };

    pub(crate) const ALL_DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[log_all_done],
        on_exit: &[],
        transitions: &[],
    };
}
