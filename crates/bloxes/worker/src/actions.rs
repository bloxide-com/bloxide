// Copyright 2025 Bloxide, all rights reserved
//! Action functions for the Worker actor.
use bloxide_core::{capability::BloxRuntime, transition::ActionResult};
use bloxide_peers::apply_peer_control;
use pool_messages::WorkerMsg;

use crate::{WorkerCtx, WorkerEvent, WorkerSpec};

impl<R: BloxRuntime> WorkerSpec<R> {
    pub(crate) fn handle_ctrl(ctx: &mut WorkerCtx<R>, ev: &WorkerEvent<R>) -> ActionResult {
        if let Some(ctrl) = ev.ctrl_payload() {
            apply_peer_control(&mut ctx.peers, ctrl);
        }
        ActionResult::Ok
    }

    pub(crate) fn process_work(ctx: &mut WorkerCtx<R>, ev: &WorkerEvent<R>) -> ActionResult {
        if let Some(WorkerMsg::DoWork(do_work)) = ev.msg_payload() {
            ctx.task_id = do_work.task_id;
            ctx.result = do_work.task_id * 2;
        }
        ActionResult::Ok
    }

    pub(crate) fn do_broadcast(ctx: &mut WorkerCtx<R>, _ev: &WorkerEvent<R>) -> ActionResult {
        bloxide_peers::broadcast_to_peers(ctx.self_id, &ctx.peers, ctx.result);
        ActionResult::Ok
    }

    pub(crate) fn do_notify_pool(ctx: &mut WorkerCtx<R>, _ev: &WorkerEvent<R>) -> ActionResult {
        blox_ctx_pool_ref::notify_pool_done(ctx.self_id, &ctx.pool_ref, ctx.task_id, ctx.result);
        ActionResult::Ok
    }
}
