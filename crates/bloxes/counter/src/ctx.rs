// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::ActorId;
use bloxide_macros::BloxCtx;
use counter_actions::{CountsTicks, __delegate_CountsTicks};

/// Context for the counter blox.
#[derive(BloxCtx)]
pub struct CounterCtx<B: CountsTicks> {
    pub self_id: ActorId,

    #[delegates(CountsTicks)]
    pub behavior: B,
}
