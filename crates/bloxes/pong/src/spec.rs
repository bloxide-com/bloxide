// Copyright 2025 Bloxide, all rights reserved
use crate::pong_state_handler_table;
use crate::{PongCtx, PongEvent};
use bloxide_core::{
    capability::BloxRuntime,
    spec::{MachineSpec, StateFns},
    transitions, HasSelfId,
};
use ping_pong_actions::send_pong;
use ping_pong_messages::PingPongMsg;

pub use crate::generated::topology::PongState;

pub struct PongSpec<R: BloxRuntime>(core::marker::PhantomData<R>);

impl<R: BloxRuntime> PongSpec<R> {
    fn reply_pong_action(
        ctx: &mut PongCtx<R>,
        ev: &PongEvent,
    ) -> bloxide_core::transition::ActionResult {
        if let Some(PingPongMsg::Ping(ping)) = ev.msg_payload() {
            bloxide_log::blox_log_info!(
                ctx.self_id(),
                "Ping({}) received — sending Pong",
                ping.round
            );
            return send_pong::<R, _>(ctx, ping);
        }
        bloxide_core::transition::ActionResult::Ok
    }

    fn log_error(ctx: &mut PongCtx<R>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "entered error state — send_pong failed");
    }

    const READY_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PingPongMsg::Ping(_ping) => {
                actions [Self::reply_pong_action]
                guard(_ctx, results) {
                    results.any_failed() => PongState::Error,
                    _ => stay,
                }
            },
        ],
    };

    const ERROR_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::log_error],
        on_exit: &[],
        transitions: &[],
    };
}

impl<R: BloxRuntime> MachineSpec for PongSpec<R> {
    type State = PongState;
    type Event = PongEvent;
    type Ctx = PongCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<PingPongMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = pong_state_handler_table!(Self);

    fn initial_state() -> PongState {
        PongState::Ready
    }

    fn is_error(state: &PongState) -> bool {
        matches!(state, PongState::Error)
    }

    fn on_init_entry(ctx: &mut PongCtx<R>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "reset");
    }
}
