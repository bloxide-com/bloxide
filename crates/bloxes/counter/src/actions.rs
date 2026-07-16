// Copyright 2025 Bloxide, all rights reserved
use crate::prelude::*;
use blox_ctx_ticks::CountsTicks;
use bloxide_core::{
    spec::StateFns, transition::ActionResult, transitions,
};
use counter_actions::increment_count;
use counter_messages::CounterMsg;

const DONE_AT_COUNT: u8 = 2;

impl<B: CountsTicks + 'static> CounterSpec<B> {
    fn count_tick(ctx: &mut CounterCtx<B>, _ev: &CounterEvent) -> ActionResult {
        increment_count(ctx);
        ActionResult::Ok
    }
}

impl<B: CountsTicks + 'static> CounterSpec<B> {
    pub const READY_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            CounterMsg::Tick(_tick) => {
                actions [Self::count_tick]
                guard(ctx, _results) {
                    ctx.count() >= B::Count::from(DONE_AT_COUNT) => CounterState::Done,
                    _ => stay,
                }
            },
        ],
    };

    pub const DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: &[],
    };
}
