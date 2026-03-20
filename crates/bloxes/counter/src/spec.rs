// Copyright 2025 Bloxide, all rights reserved
use core::marker::PhantomData;

use bloxide_core::{
    capability::BloxRuntime,
    spec::{MachineSpec, StateFns},
    transition::ActionResult,
    transitions,
};
use bloxide_macros::StateTopology;
use counter_actions::{increment_count, CountsTicks};
use counter_messages::CounterMsg;

use crate::{CounterCtx, CounterEvent};

const DONE_AT_COUNT: u8 = 2;

#[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
#[handler_fns(READY_FNS, DONE_FNS)]
pub enum CounterState {
    Ready,
    Done,
}

pub struct CounterSpec<R, B>(PhantomData<(R, B)>)
where
    R: BloxRuntime,
    B: CountsTicks + 'static;

impl<R, B> CounterSpec<R, B>
where
    R: BloxRuntime,
    B: CountsTicks + 'static,
{
    fn count_tick(ctx: &mut CounterCtx<B>, _ev: &CounterEvent) -> ActionResult {
        increment_count(ctx);
        ActionResult::Ok
    }

    const READY_FNS: StateFns<Self> = StateFns {
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

    const DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: &[],
    };
}

impl<R, B> MachineSpec for CounterSpec<R, B>
where
    R: BloxRuntime,
    B: CountsTicks + 'static,
{
    type State = CounterState;
    type Event = CounterEvent;
    type Ctx = CounterCtx<B>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<CounterMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = counter_state_handler_table!(Self);

    fn initial_state() -> CounterState {
        CounterState::Ready
    }

    fn is_terminal(state: &CounterState) -> bool {
        matches!(state, CounterState::Done)
    }

    fn on_init_entry(ctx: &mut CounterCtx<B>) {
        ctx.set_count(B::Count::from(0));
    }
}
