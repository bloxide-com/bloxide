// Copyright 2025 Bloxide, all rights reserved
//! Action functions for the Worker actor.
use blox_ctx_current_task::HasCurrentTask;
use bloxide_core::{accessor::HasSelfId, capability::BloxRuntime, transition::ActionResult};
use bloxide_peers::HasPeers;
use pool_actions::actions::{apply_worker_control, broadcast_to_peers, notify_pool_done};
use pool_messages::WorkerMsg;

use crate::{WorkerCtx, WorkerEvent, WorkerSpec};

impl<R: BloxRuntime, B: HasPeers<WorkerMsg, R> + HasCurrentTask + 'static> WorkerSpec<R, B> {
    pub(crate) fn handle_ctrl(ctx: &mut WorkerCtx<R, B>, ev: &WorkerEvent<R>) -> ActionResult {
        if let Some(ctrl) = ev.ctrl_payload() {
            apply_worker_control(ctx, ctrl);
        }
        ActionResult::Ok
    }

    pub(crate) fn process_work(ctx: &mut WorkerCtx<R, B>, ev: &WorkerEvent<R>) -> ActionResult {
        if let Some(WorkerMsg::DoWork(do_work)) = ev.msg_payload() {
            ctx.set_task_id(do_work.task_id);
            ctx.set_result(do_work.task_id * 2);
        }
        ActionResult::Ok
    }

    pub(crate) fn do_broadcast(ctx: &mut WorkerCtx<R, B>) {
        broadcast_to_peers::<R, _>(ctx);
    }

    pub(crate) fn do_notify_pool(ctx: &mut WorkerCtx<R, B>) {
        notify_pool_done::<R, _>(ctx);
    }

    pub(crate) fn log_waiting(ctx: &mut WorkerCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "worker waiting for task");
    }

    pub(crate) fn log_done(ctx: &mut WorkerCtx<R, B>) {
        bloxide_log::blox_log_info!(
            ctx.self_id(),
            "worker done: task_id={} result={}",
            ctx.task_id(),
            ctx.result()
        );
    }
}
