// Copyright 2025 Bloxide, all rights reserved
use crate::prelude::*;
use bloxide_core::{
    capability::BloxRuntime, spec::StateFns, transition::ActionResult, transitions,
};
use ping_pong_actions::send_pong;
use ping_pong_messages::PingPongMsg;

impl<R: BloxRuntime> PongSpec<R> {
    fn reply_pong_action(ctx: &mut PongCtx<R>, ev: &PongEvent) -> ActionResult {
        if let Some(PingPongMsg::Ping(ping)) = ev.msg_payload() {
            return send_pong::<R, _>(ctx, ping);
        }
        ActionResult::Ok
    }

    pub const READY_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PingPongMsg::Ping(_) => {
                actions [Self::reply_pong_action]
                stay
            },
        ],
    };
}
