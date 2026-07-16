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
        }
    }
    ActionResult::Ok
}

/// Handle a SpawnedWorker reply: store the worker refs, introduce peers,
/// send DoWork, and transition to Active.
pub fn handle_spawned_worker<R: BloxRuntime>(
    ctx: &mut PoolCtx<R>,
    ev: &PoolEvent<R>,
) -> ActionResult {
    if let Some(spawned) = ev.spawn_reply_payload() {
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
                transition PoolState::Active
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
                    ctx.pending() == 0 => PoolState::AllDone,
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
