// Copyright 2025 Bloxide, all rights reserved
use core::marker::PhantomData;

use bloxide_core::{
    capability::BloxRuntime,
    spec::{MachineSpec, StateFns},
    transition::ActionResult,
    transitions,
};
use bhsm_tst_messages::BhsmTstMsg;

use crate::bhsm_tst_state_handler_table;
use crate::{BhsmTstCtx, BhsmTstEvent};

pub use crate::generated::topology::BhsmTstState;

#[cfg(feature = "std")]
macro_rules! trace {
    ($($arg:tt)*) => { std::println!($($arg)*); };
}
#[cfg(not(feature = "std"))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

pub struct BhsmTstSpec<R: BloxRuntime>(PhantomData<R>);

impl<R: BloxRuntime> BhsmTstSpec<R> {
    fn s_entry(_ctx: &mut BhsmTstCtx) { trace!("s-ENTRY;"); }
    fn s_exit(_ctx: &mut BhsmTstCtx) { trace!("s-EXIT;"); }
    fn s1_entry(_ctx: &mut BhsmTstCtx) { trace!("s1-ENTRY;"); }
    fn s1_exit(_ctx: &mut BhsmTstCtx) { trace!("s1-EXIT;"); }
    fn s11_entry(_ctx: &mut BhsmTstCtx) { trace!("s11-ENTRY;"); }
    fn s11_exit(_ctx: &mut BhsmTstCtx) { trace!("s11-EXIT;"); }
    fn s2_entry(_ctx: &mut BhsmTstCtx) { trace!("s2-ENTRY;"); }
    fn s2_exit(_ctx: &mut BhsmTstCtx) { trace!("s2-EXIT;"); }
    fn s21_entry(_ctx: &mut BhsmTstCtx) { trace!("s21-ENTRY;"); }
    fn s21_exit(_ctx: &mut BhsmTstCtx) { trace!("s21-EXIT;"); }
    fn s211_entry(_ctx: &mut BhsmTstCtx) { trace!("s211-ENTRY;"); }
    fn s211_exit(_ctx: &mut BhsmTstCtx) { trace!("s211-EXIT;"); }
    fn error_entry(_ctx: &mut BhsmTstCtx) { trace!("error-ENTRY;"); }
    fn error_exit(_ctx: &mut BhsmTstCtx) { trace!("error-EXIT;"); }
    fn done_entry(_ctx: &mut BhsmTstCtx) { trace!("done-ENTRY;"); }
    fn done_exit(_ctx: &mut BhsmTstCtx) { trace!("done-EXIT;"); }

    fn s_i(_ctx: &mut BhsmTstCtx, _ev: &BhsmTstEvent) -> ActionResult {
        trace!("s-I;");
        ActionResult::Ok
    }

    fn s11_a(_ctx: &mut BhsmTstCtx, _ev: &BhsmTstEvent) -> ActionResult {
        trace!("s11-A;");
        ActionResult::Ok
    }

    fn s11_b(_ctx: &mut BhsmTstCtx, _ev: &BhsmTstEvent) -> ActionResult {
        trace!("s11-B;");
        ActionResult::Ok
    }

    const S_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s_entry],
        on_exit: &[Self::s_exit],
        transitions: transitions![
            BhsmTstMsg::H(_) => {
                transition BhsmTstState::S11
            },
            BhsmTstMsg::I(_) => {
                actions [Self::s_i]
                guard(_ctx, _results) {
                    _ => stay,
                }
            },
            BhsmTstMsg::K(_) => {
                transition BhsmTstState::Error
            },
            BhsmTstMsg::X(_) => {
                transition BhsmTstState::Done
            },
        ],
    };

    const S1_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s1_entry],
        on_exit: &[Self::s1_exit],
        transitions: transitions![
            BhsmTstMsg::C(_) => {
                transition BhsmTstState::S211
            },
        ],
    };

    const S11_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s11_entry],
        on_exit: &[Self::s11_exit],
        transitions: transitions![
            BhsmTstMsg::A(_) => {
                actions [Self::s11_a]
                transition BhsmTstState::S11
            },
            BhsmTstMsg::B(_) => {
                actions [Self::s11_b]
                transition BhsmTstState::S11
            },
            BhsmTstMsg::D(_) => {
                transition BhsmTstState::S211
            },
        ],
    };

    const S2_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s2_entry],
        on_exit: &[Self::s2_exit],
        transitions: &[],
    };

    const S21_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s21_entry],
        on_exit: &[Self::s21_exit],
        transitions: transitions![
            BhsmTstMsg::E(_) => {
                transition BhsmTstState::S211
            },
            BhsmTstMsg::G(_) => {
                transition BhsmTstState::S11
            },
        ],
    };

    const S211_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s211_entry],
        on_exit: &[Self::s211_exit],
        transitions: transitions![
            BhsmTstMsg::F(_) => {
                transition BhsmTstState::S11
            },
        ],
    };

    const ERROR_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::error_entry],
        on_exit: &[Self::error_exit],
        transitions: &[],
    };

    const DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::done_entry],
        on_exit: &[Self::done_exit],
        transitions: &[],
    };
}

impl<R: BloxRuntime> MachineSpec for BhsmTstSpec<R> {
    type State = BhsmTstState;
    type Event = BhsmTstEvent;
    type Ctx = BhsmTstCtx;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<BhsmTstMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = bhsm_tst_state_handler_table!(Self);

    fn initial_state() -> BhsmTstState {
        BhsmTstState::S11
    }

    fn is_terminal(state: &BhsmTstState) -> bool {
        matches!(state, BhsmTstState::Done)
    }

    fn is_error(state: &BhsmTstState) -> bool {
        matches!(state, BhsmTstState::Error)
    }

    fn on_init_entry(_ctx: &mut BhsmTstCtx) {}
}
