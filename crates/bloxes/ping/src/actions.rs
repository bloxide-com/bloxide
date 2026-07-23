// Copyright 2025 Bloxide, all rights reserved
use crate::{PingCtx, PingEvent, PingSpec, PAUSE_DURATION_MS};
use blox_ctx_current_timer::HasCurrentTimer;
use blox_ctx_rounds::CountsRounds;
use bloxide_core::{capability::BloxRuntime, transition::ActionResult};

impl<R, B> PingSpec<R, B>
where
    R: BloxRuntime,
    B: HasCurrentTimer + CountsRounds + Default + 'static,
    B::Round: Into<u32>,
{
    pub(crate) fn increment_round(ctx: &mut PingCtx<R, B>) {
        let one = B::Round::from(1);
        ctx.behavior.set_round(ctx.behavior.round() + one);
    }

    pub(crate) fn send_initial_ping(ctx: &mut PingCtx<R, B>) {
        bloxide_messaging::send_initial_ping::<R>(
            ctx.self_id,
            &ctx.peer_ref,
            ctx.behavior.round().into(),
        );
    }

    pub(crate) fn forward_ping(ctx: &mut PingCtx<R, B>, _ev: &PingEvent) -> ActionResult {
        bloxide_messaging::send_ping::<R>(
            ctx.self_id,
            &ctx.peer_ref,
            ctx.behavior.round().into(),
        )
    }

    pub(crate) fn schedule_pause_timer(ctx: &mut PingCtx<R, B>) {
        let id = blox_ctx_current_timer::schedule_resume::<R>(
            ctx.self_id,
            &ctx.self_ref,
            &ctx.timer_ref,
            PAUSE_DURATION_MS,
        );
        ctx.behavior.set_current_timer(Some(id));
    }

    pub(crate) fn cancel_pause_timer(ctx: &mut PingCtx<R, B>) {
        blox_ctx_current_timer::cancel_timer_by_id::<R>(
            ctx.self_id,
            &ctx.timer_ref,
            ctx.behavior.current_timer(),
        );
        ctx.behavior.set_current_timer(None);
    }
}
