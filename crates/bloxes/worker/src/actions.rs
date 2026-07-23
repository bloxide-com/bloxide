// Copyright 2025 Bloxide, all rights reserved
use blox_ctx_current_task::HasCurrentTask;
use bloxide_core::{capability::BloxRuntime, transition::ActionResult};
use bloxide_peers::HasPeers;
use pool_messages::WorkerMsg;

use crate::{WorkerCtx, WorkerEvent, WorkerSpec};

impl<R: BloxRuntime, B: HasPeers<WorkerMsg, R> + HasCurrentTask + 'static> WorkerSpec<R, B> {
    pub(crate) fn handle_ctrl(ctx: &mut WorkerCtx<R, B>, ev: &WorkerEvent<R>) -> ActionResult {
        if let Some(ctrl) = ev.ctrl_payload() {
            bloxide_peers::apply_peer_control(ctx.peers_mut(), ctrl);
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

    pub(crate) fn do_broadcast(ctx: &mut WorkerCtx<R, B>, _ev: &WorkerEvent<R>) -> ActionResult {
        bloxide_peers::broadcast_to_peers::<R>(
            ctx.self_id,
            &ctx.peers(),
            ctx.result(),
        );
        ActionResult::Ok
    }

    pub(crate) fn do_notify_pool(ctx: &mut WorkerCtx<R, B>, _ev: &WorkerEvent<R>) -> ActionResult {
        blox_ctx_pool_ref::notify_pool_done::<R>(
            ctx.self_id,
            &ctx.pool_ref,
            ctx.task_id(),
            ctx.result(),
        );
        ActionResult::Ok
    }
}
