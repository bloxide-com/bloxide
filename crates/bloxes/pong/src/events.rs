// Copyright 2025 Bloxide, all rights reserved
//! Unified event type for the Pong actor.
use bloxide_macros::event;
use ping_pong_messages::PingPongMsg;

event!(Pong { Msg: PingPongMsg });
