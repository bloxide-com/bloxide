// Copyright 2025 Bloxide, all rights reserved
//! Pool action functions and state handler tables.
use bloxide_core::{
    capability::BloxRuntime,
    messaging::ActorRef,
    spec::StateFns,
    transition::ActionResult,
    transitions,
    HasSelfId,
};
use alloc::vec::Vec;
use pool_actions::{
    actions::introduce_new_worker,
    traits::{HasWorkerFactory, HasWorkers, WorkerSpawnFn},
};
use pool_messages::{DoWork, PoolMsg, SpawnWorker, WorkerCtrl, WorkerMsg};

pub use crate::generated::topology::PoolState;
use crate::{PoolCtx, PoolEvent, PoolSpec};

pub fn spawn_worker<R: BloxRuntime>(ctx: &mut PoolCtx<R>, task_id: u32) {
    let self_id = ctx.self_id();
    let (domain_ref, ctrl_ref) = (ctx.worker_factory)(self_id, &ctx.self_ref);
    ctx.worker_refs_mut().push(domain_ref.clone());
    ctx.worker_ctrls_mut().push(ctrl_ref);
    ctx.set_pending(ctx.pending() + 1);
    introduce_new_worker(ctx);
    if domain_ref
        .try_send(self_id, WorkerMsg::DoWork(DoWork { task_id }))
        .is_err()
    {
        bloxide_log::blox_log_warn!(self_id, "worker channel full, dropping task_id={}", task_id);
        if ctx.pending() > 0 {
            ctx.set_pending(ctx.pending() - 1);
        }
    }
}

pub fn handle_spawn_worker<R: BloxRuntime>(ctx: &mut PoolCtx<R>, ev: &PoolEvent) -> ActionResult {
    if let Some(PoolMsg::SpawnWorker(SpawnWorker { task_id })) = ev.msg_payload() {
        bloxide_log::blox_log_info!(ctx.self_id(), "spawning worker for task_id={}", task_id);
        spawn_worker(ctx, *task_id);
    }
    ActionResult::Ok
}

pub fn handle_work_done<R: BloxRuntime>(ctx: &mut PoolCtx<R>, ev: &PoolEvent) -> ActionResult {
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

impl<R: BloxRuntime> HasWorkerFactory<R> for PoolCtx<R> {
    fn worker_factory(&self) -> WorkerSpawnFn<R> {
        self.worker_factory
    }
}

impl<R: BloxRuntime> HasWorkers<R> for PoolCtx<R> {
    fn worker_refs(&self) -> &[ActorRef<WorkerMsg, R>] {
        &self.worker_refs
    }
    fn worker_refs_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>> {
        &mut self.worker_refs
    }
    fn worker_ctrls(&self) -> &[ActorRef<WorkerCtrl<R>, R>] {
        &self.worker_ctrls
    }
    fn worker_ctrls_mut(&mut self) -> &mut Vec<ActorRef<WorkerCtrl<R>, R>> {
        &mut self.worker_ctrls
    }
    fn pending(&self) -> u32 {
        self.pending
    }
    fn set_pending(&mut self, n: u32) {
        self.pending = n;
    }
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
