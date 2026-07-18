// Copyright 2025 Bloxide, all rights reserved
use crate::{PingCtx, PingEvent, PingSpec, PAUSE_DURATION_MS};
use blox_ctx_current_timer::HasCurrentTimer;
use blox_ctx_rounds::CountsRounds;
use bloxide_core::{capability::BloxRuntime, transition::ActionResult, HasSelfId};
use ping_pong_actions::{cancel_current_timer, schedule_resume, send_ping};
use ping_pong_messages::PingPongMsg;

impl<R, B> PingSpec<R, B>
where
    R: BloxRuntime,
    B: HasCurrentTimer + CountsRounds + Default + 'static,
    B::Round: Into<u32>,
{
    pub(crate) fn log_pong_received(ctx: &mut PingCtx<R, B>, ev: &PingEvent) -> ActionResult {
        if let Some(PingPongMsg::Pong(pong)) = ev.msg_payload() {
            bloxide_log::blox_log_debug!(ctx.self_id(), "Pong({}) received", pong.round);
        }
        ActionResult::Ok
    }

    pub(crate) fn forward_ping(ctx: &mut PingCtx<R, B>, _ev: &PingEvent) -> ActionResult {
        send_ping::<R, _>(ctx)
    }

    pub(crate) fn schedule_pause_timer(ctx: &mut PingCtx<R, B>) {
        schedule_resume::<R, _>(ctx, PAUSE_DURATION_MS);
        bloxide_log::blox_log_info!(
            ctx.self_id(),
            "paused — resuming in {}ms",
            PAUSE_DURATION_MS
        );
    }

    pub(crate) fn cancel_pause_timer(ctx: &mut PingCtx<R, B>) {
        cancel_current_timer::<R, _>(ctx);
    }

    pub(crate) fn log_round(ctx: &mut PingCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "round {} — sending Ping", ctx.round());
    }

    pub(crate) fn log_done(ctx: &mut PingCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "done after {} rounds", ctx.round());
    }

    pub(crate) fn log_error(ctx: &mut PingCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "entered error state");
    }
}
