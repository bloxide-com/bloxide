// Copyright 2025 Bloxide, all rights reserved
use crate::prelude::*;
use bloxide_core::{capability::BloxRuntime, transition::ActionResult};
use ping_pong_actions::send_pong;
use ping_pong_messages::PingPongMsg;

impl<R: BloxRuntime> PongSpec<R> {
    pub(crate) fn reply_pong_action(ctx: &mut PongCtx<R>, ev: &PongEvent) -> ActionResult {
        if let Some(PingPongMsg::Ping(ping)) = ev.msg_payload() {
            return send_pong::<R, _>(ctx, ping);
        }
        ActionResult::Ok
    }
}
