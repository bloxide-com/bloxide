// Copyright 2025 Bloxide, all rights reserved
//! Action functions and StateFns constants for the Worker actor.
//!
//! Moved from `spec.rs` during migration to TOML-driven structure.
use blox_ctx_current_task::HasCurrentTask;
use blox_ctx_worker_peers::HasWorkerPeers;
use bloxide_core::{
    accessor::HasSelfId, capability::BloxRuntime, spec::StateFns, transition::ActionResult,
    transitions,
};
use pool_actions::actions::{apply_worker_control, broadcast_to_peers, notify_pool_done};
use pool_messages::{WorkerCtrl, WorkerMsg};

use crate::{WorkerCtx, WorkerEvent, WorkerSpec, WorkerState};

impl<R: BloxRuntime, B: HasWorkerPeers<R> + HasCurrentTask + 'static> WorkerSpec<R, B> {
    fn handle_ctrl(ctx: &mut WorkerCtx<R, B>, ev: &WorkerEvent<R>) -> ActionResult {
        if let Some(ctrl) = ev.ctrl_payload() {
            apply_worker_control(ctx, ctrl);
        }
        ActionResult::Ok
    }

    fn process_work(ctx: &mut WorkerCtx<R, B>, ev: &WorkerEvent<R>) -> ActionResult {
        if let Some(WorkerMsg::DoWork(do_work)) = ev.msg_payload() {
            ctx.set_task_id(do_work.task_id);
            ctx.set_result(do_work.task_id * 2);
        }
        ActionResult::Ok
    }

    fn do_broadcast(ctx: &mut WorkerCtx<R, B>) {
        broadcast_to_peers::<R, _>(ctx);
    }

    fn do_notify_pool(ctx: &mut WorkerCtx<R, B>) {
        notify_pool_done::<R, _>(ctx);
    }

    fn log_waiting(ctx: &mut WorkerCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "worker waiting for task");
    }

    fn log_done(ctx: &mut WorkerCtx<R, B>) {
        bloxide_log::blox_log_info!(
            ctx.self_id(),
            "worker done: task_id={} result={}",
            ctx.task_id(),
            ctx.result()
        );
    }

    pub(crate) const WAITING_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::log_waiting],
        on_exit: &[],
        transitions: transitions![
            WorkerCtrl::AddPeer(_) | WorkerCtrl::RemovePeer(_) => {
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

    pub(crate) const DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::log_done, Self::do_broadcast, Self::do_notify_pool],
        on_exit: &[],
        transitions: &[],
    };
}
