// Copyright 2025 Bloxide, all rights reserved
use crate::prelude::*;
use blox_ctx_ticks::CountsTicks;
use bloxide_core::transition::ActionResult;
use counter_actions::increment_count;

impl<B: CountsTicks + 'static> CounterSpec<B> {
    pub(crate) fn count_tick(ctx: &mut CounterCtx<B>, _ev: &CounterEvent) -> ActionResult {
        increment_count(ctx);
        ActionResult::Ok
    }
}
