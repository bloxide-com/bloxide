// Copyright 2025 Bloxide, all rights reserved
//! Pool action functions and state handler tables.
use bloxide_core::{
    capability::BloxRuntime,
    spec::StateFns,
    transition::ActionResult,
    transitions, HasSelfId,
};
use pool_actions::{actions::introduce_new_worker, traits::HasWorkers};
use pool_messages::{DoWork, PoolMsg, SpawnWorker, WorkerMsg};

use crate::{PoolCtx, PoolEvent, PoolSpec};
pub use crate::generated::topology::PoolState;

pub fn spawn_worker<R: BloxRuntime>(ctx: &mut PoolCtx<R>, task_id: u32) {
    let self_id = ctx.self_id();
    let (domain_ref, ctrl_ref) = (ctx.worker_factory)(self_id, &ctx.self_ref);
    ctx.worker_refs_mut().push(domain_ref.clone());
    ctx.worker_ctrls_mut().push(ctrl_ref);
    ctx.set_pending(ctx.pending() + 1);
    introduce_new_worker(ctx);
    let _ = domain_ref.try_send(self_id, WorkerMsg::DoWork(DoWork { task_id }));
}

pub fn handle_spawn_worker<R: BloxRuntime>(
    ctx: &mut PoolCtx<R>,
    ev: &PoolEvent,
) -> ActionResult {
    if let Some(PoolMsg::SpawnWorker(SpawnWorker { task_id })) = ev.msg_payload() {
        bloxide_log::blox_log_info!(ctx.self_id(), "spawning worker for task_id={}", task_id);
        spawn_worker(ctx, *task_id);
    }
    ActionResult::Ok
}

pub fn handle_work_done<R: BloxRuntime>(
    ctx: &mut PoolCtx<R>,
    ev: &PoolEvent,
) -> ActionResult {
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
                stay
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
