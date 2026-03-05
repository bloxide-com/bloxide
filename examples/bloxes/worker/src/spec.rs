// Copyright 2025 Bloxide, all rights reserved
//! State machine specification for the Worker actor.
//!
//! State topology:
//! ```text
//! [VirtualRoot — engine implicit]
//! ├── Waiting  (leaf, initial)  — accumulates peers, awaits DoWork
//! └── Done     (leaf, terminal) — broadcasts result, notifies pool
//! ```
use core::marker::PhantomData;

use bloxide_core::{
    accessor::HasSelfId,
    capability::BloxRuntime,
    spec::{MachineSpec, StateFns},
    transition::ActionResult,
    transitions,
};
use bloxide_macros::StateTopology;
use bloxide_spawn::{
    apply_peer_ctrl,
    peer::{HasPeers, PeerCtrl},
};
use pool_actions::{
    actions::{broadcast_to_peers, notify_pool_done},
    traits::HasCurrentTask,
};
use pool_messages::WorkerMsg;

use crate::{WorkerCtx, WorkerEvent};

/// Worker state topology — two flat leaf states.
#[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
#[handler_fns(WAITING_FNS, DONE_FNS)]
pub enum WorkerState {
    /// Initial operational state: accepts peer introductions and waits for DoWork.
    Waiting,
    /// Terminal state: broadcasts result to peers, notifies pool.
    Done,
}

pub struct WorkerSpec<R: BloxRuntime>(PhantomData<R>);

impl<R: BloxRuntime> WorkerSpec<R> {
    /// Apply an incoming `PeerCtrl` command (AddPeer / RemovePeer) to the peer list.
    fn handle_ctrl(ctx: &mut WorkerCtx<R>, ev: &WorkerEvent<R>) -> ActionResult {
        if let Some(ctrl) = ev.ctrl_payload() {
            apply_peer_ctrl(ctx, ctrl);
        }
        ActionResult::Ok
    }

    /// Set task_id and compute result from an incoming `DoWork` message.
    fn process_work(ctx: &mut WorkerCtx<R>, ev: &WorkerEvent<R>) -> ActionResult {
        if let Some(WorkerMsg::DoWork(do_work)) = ev.msg_payload() {
            ctx.set_task_id(do_work.task_id);
            // Demonstration computation: result = task_id * 2
            ctx.set_result(do_work.task_id * 2);
        }
        ActionResult::Ok
    }

    fn do_broadcast(ctx: &mut WorkerCtx<R>) {
        broadcast_to_peers::<R, _>(ctx);
    }

    fn do_notify_pool(ctx: &mut WorkerCtx<R>) {
        notify_pool_done::<R, _>(ctx);
    }

    fn log_waiting(ctx: &mut WorkerCtx<R>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "worker waiting for task");
    }

    fn log_done(ctx: &mut WorkerCtx<R>) {
        bloxide_log::blox_log_info!(
            ctx.self_id(),
            "worker done: task_id={} result={}",
            ctx.task_id(),
            ctx.result()
        );
    }

    const WAITING_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::log_waiting],
        on_exit: &[],
        transitions: transitions![
            PeerCtrl(_) => {
                actions [Self::handle_ctrl]
                stay
            },
            WorkerMsg::DoWork(_) => {
                actions [Self::process_work]
                transition WorkerState::Done
            },
            WorkerMsg::PeerResult(_) => stay,
        ],
    };

    const DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::log_done, Self::do_broadcast, Self::do_notify_pool],
        on_exit: &[],
        transitions: &[],
    };
}

impl<R: BloxRuntime> MachineSpec for WorkerSpec<R> {
    type State = WorkerState;
    type Event = WorkerEvent<R>;
    type Ctx = WorkerCtx<R>;

    /// Ctrl stream at index 0 (higher priority) ensures AddPeer commands are
    /// processed before DoWork arrives on the domain stream at index 1.
    ///
    /// Uses `R::Stream` (the spec's own runtime parameter) rather than `Rt::Stream`
    /// because `PeerCtrl<WorkerMsg, R>` and `WorkerEvent<R>` are parameterized by the
    /// same `R`. The `From<Envelope<PeerCtrl<WorkerMsg, R>>>` impl on `WorkerEvent<R>`
    /// requires that the stream item type matches `R`. In practice `Rt` is always
    /// bound to `R` at instantiation, so there is no observable difference, but the
    /// `Rt` parameter is technically phantom for this spec.
    type Mailboxes<Rt: BloxRuntime> = (R::Stream<PeerCtrl<WorkerMsg, R>>, R::Stream<WorkerMsg>);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = worker_state_handler_table!(Self);

    fn initial_state() -> WorkerState {
        WorkerState::Waiting
    }

    fn is_terminal(state: &WorkerState) -> bool {
        matches!(state, WorkerState::Done)
    }

    fn on_init_entry(ctx: &mut WorkerCtx<R>) {
        ctx.set_task_id(0);
        ctx.set_result(0);
        ctx.peers_mut().clear();
    }
}
