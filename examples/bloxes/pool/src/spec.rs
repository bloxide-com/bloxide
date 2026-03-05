//! State machine specification for the Pool actor.
//!
//! State topology:
//! ```text
//! [VirtualRoot — engine implicit]
//! ├── Idle     (leaf, initial)  — waiting for first SpawnWorker
//! ├── Active   (leaf)           — workers running; more SpawnWorker and WorkDone handled
//! └── AllDone  (leaf, terminal) — all workers have reported completion
//! ```
use core::marker::PhantomData;

use bloxide_core::{
    capability::BloxRuntime,
    spec::{MachineSpec, StateFns},
    transitions, HasSelfId,
};
use bloxide_macros::StateTopology;
use pool_actions::{actions::introduce_new_worker, traits::HasWorkers};
use pool_messages::{DoWork, PoolMsg, SpawnWorker, WorkerMsg};

use crate::{PoolCtx, PoolEvent};

/// Pool state topology — three flat leaf states.
#[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
#[handler_fns(IDLE_FNS, ACTIVE_FNS, ALL_DONE_FNS)]
pub enum PoolState {
    /// No workers spawned yet.
    Idle,
    /// At least one worker is running.
    Active,
    /// All workers have finished.
    AllDone,
}

pub struct PoolSpec<R>(PhantomData<R>)
where
    R: BloxRuntime;

impl<R> PoolSpec<R>
where
    R: BloxRuntime,
{
    /// Spawn a new worker for the given task, introduce it to existing workers,
    /// then send `DoWork` so the worker can begin after peer wiring is complete.
    ///
    /// The concrete worker type is not referenced here — the pool calls the
    /// factory stored in `ctx` (injected at construction time by the wiring
    /// layer). This keeps pool-blox free of any dependency on worker-blox.
    fn spawn_worker(ctx: &mut PoolCtx<R>, task_id: u32) {
        let self_id = ctx.self_id();

        // Delegate all worker construction and spawning to the injected factory.
        let (domain_ref, ctrl_ref) = (ctx.worker_factory)(self_id, &ctx.self_ref);

        // Store refs before introducing peers so introduce_new_worker sees this worker.
        ctx.worker_refs_mut().push(domain_ref.clone());
        ctx.worker_ctrls_mut().push(ctrl_ref);
        ctx.set_pending(ctx.pending() + 1);

        // Introduce the new worker to all previously spawned workers.
        introduce_new_worker(ctx);

        // Send DoWork after peer introduction — the worker's ctrl channel has higher
        // poll priority so AddPeer commands arrive before DoWork is dispatched.
        let _ = domain_ref.try_send(self_id, WorkerMsg::DoWork(DoWork { task_id }));
    }

    fn handle_spawn_worker(
        ctx: &mut PoolCtx<R>,
        ev: &PoolEvent,
    ) -> bloxide_core::transition::ActionResult {
        if let Some(PoolMsg::SpawnWorker(SpawnWorker { task_id })) = ev.msg_payload() {
            bloxide_log::blox_log_info!(ctx.self_id(), "spawning worker for task_id={}", task_id);
            Self::spawn_worker(ctx, *task_id);
        }
        bloxide_core::transition::ActionResult::Ok
    }

    fn handle_work_done(
        ctx: &mut PoolCtx<R>,
        ev: &PoolEvent,
    ) -> bloxide_core::transition::ActionResult {
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
        bloxide_core::transition::ActionResult::Ok
    }

    fn log_all_done(ctx: &mut PoolCtx<R>) {
        bloxide_log::blox_log_info!(
            ctx.self_id(),
            "all {} workers done",
            ctx.worker_refs().len()
        );
    }

    const IDLE_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PoolMsg::SpawnWorker(_) => {
                actions [Self::handle_spawn_worker]
                transition PoolState::Active
            },
        ],
    };

    const ACTIVE_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PoolMsg::SpawnWorker(_) => {
                actions [Self::handle_spawn_worker]
                stay
            },
            PoolMsg::WorkDone(_done) => {
                actions [Self::handle_work_done]
                guard(ctx, _results) {
                    ctx.pending() == 0 => PoolState::AllDone,
                    _ => stay,
                }
            },
        ],
    };

    const ALL_DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::log_all_done],
        on_exit: &[],
        transitions: &[],
    };
}

impl<R> MachineSpec for PoolSpec<R>
where
    R: BloxRuntime,
{
    type State = PoolState;
    type Event = PoolEvent;
    type Ctx = PoolCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<PoolMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = pool_state_handler_table!(Self);

    fn initial_state() -> PoolState {
        PoolState::Idle
    }

    fn is_terminal(state: &PoolState) -> bool {
        matches!(state, PoolState::AllDone)
    }

    fn on_init_entry(ctx: &mut PoolCtx<R>) {
        ctx.worker_refs_mut().clear();
        ctx.worker_ctrls_mut().clear();
        ctx.set_pending(0);
    }
}
