// Copyright 2025 Bloxide, all rights reserved
//! Action functions for the Counter actor.
use bloxide_core::transition::ActionResult;

use crate::{CounterCtx, CounterEvent, CounterSpec};

impl CounterSpec {
    pub(crate) fn count_tick(ctx: &mut CounterCtx, _ev: &CounterEvent) -> ActionResult {
        ctx.count += 1;
        ActionResult::Ok
    }
}
