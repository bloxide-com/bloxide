// Copyright 2025 Bloxide, all rights reserved
use crate::{PingCtx, PingEvent, PingSpec, MAX_ROUNDS, PAUSE_AT_ROUND, PAUSE_DURATION_MS};
use bloxide_core::{
    capability::BloxRuntime,
    spec::StateFns,
    transition::ActionResult,
    transitions, HasSelfId,
};
use ping_pong_actions::{
    cancel_current_timer, increment_round, schedule_resume, send_initial_ping, send_ping,
    CountsRounds, HasCurrentTimer,
};
use ping_pong_messages::PingPongMsg;

use crate::PingState;

impl<R, B> PingSpec<R, B>
where
    R: BloxRuntime,
    B: HasCurrentTimer + CountsRounds + Default + 'static,
    B::Round: Into<u32>,
{
    fn log_pong_received(ctx: &mut PingCtx<R, B>, ev: &PingEvent) -> ActionResult {
        if let Some(PingPongMsg::Pong(pong)) = ev.msg_payload() {
            bloxide_log::blox_log_debug!(ctx.self_id(), "Pong({}) received", pong.round);
        }
        ActionResult::Ok
    }

    fn forward_ping(ctx: &mut PingCtx<R, B>, _ev: &PingEvent) -> ActionResult {
        send_ping::<R, _>(ctx)
    }

    fn schedule_pause_timer(ctx: &mut PingCtx<R, B>) {
        schedule_resume::<R, _>(ctx, PAUSE_DURATION_MS);
        bloxide_log::blox_log_info!(
            ctx.self_id(),
            "paused — resuming in {}ms",
            PAUSE_DURATION_MS
        );
    }

    fn cancel_pause_timer(ctx: &mut PingCtx<R, B>) {
        cancel_current_timer::<R, _>(ctx);
    }

    fn log_round(ctx: &mut PingCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "round {} — sending Ping", ctx.round());
    }

    fn log_done(ctx: &mut PingCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "done after {} rounds", ctx.round());
    }

    fn log_error(ctx: &mut PingCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "entered error state");
    }

    pub(crate) const OPERATING_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PingPongMsg::Pong(_) => stay,
        ],
    };

    pub(crate) const ACTIVE_FNS: StateFns<Self> = StateFns {
        on_entry: &[increment_round, Self::log_round, send_initial_ping],
        on_exit: &[],
        transitions: transitions![
            PingPongMsg::Pong(_) => {
                actions [Self::log_pong_received, Self::forward_ping]
                guard(ctx, results) {
                    results.any_failed()                          => PingState::Error,
                    ctx.round() >= B::Round::from(MAX_ROUNDS)     => PingState::Done,
                    ctx.round() == B::Round::from(PAUSE_AT_ROUND) => PingState::Paused,
                    _                                             => PingState::Active,
                }
            },
        ],
    };

    pub(crate) const PAUSED_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::schedule_pause_timer],
        on_exit: &[Self::cancel_pause_timer],
        transitions: transitions![
            PingPongMsg::Resume(_resume) => {
                actions [Self::forward_ping]
                transition PingState::Active
            },
        ],
    };

    pub(crate) const DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::log_done],
        on_exit: &[],
        transitions: &[],
    };

    pub(crate) const ERROR_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::log_error],
        on_exit: &[],
        transitions: &[],
    };
}
