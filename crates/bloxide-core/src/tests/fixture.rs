// Copyright 2025 Bloxide, all rights reserved
use crate::engine::StateMachine;
use crate::event_tag::LifecycleEvent;
use crate::lifecycle::LifecycleCommand;
use crate::spec::{MachineSpec, StateFns};
use crate::topology::LeafState;
use crate::transition::{ActionResult, Decision, StateRule};
use std::cell::RefCell;
use std::thread_local;
use std::vec::Vec;

thread_local! {
    static LOG: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

pub fn log(msg: &'static str) {
    LOG.with(|l| l.borrow_mut().push(msg));
}

pub fn take_log() -> Vec<&'static str> {
    LOG.with(|l| {
        let mut v = l.borrow_mut();
        let out = v.clone();
        v.clear();
        out
    })
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub enum TState {
    Top,
    #[default]
    A,
    B,
    Other,
    C,
}

impl crate::topology::StateTopology for TState {
    const STATE_COUNT: usize = 5;

    fn parent(self) -> Option<Self> {
        match self {
            TState::Top | TState::Other => None,
            TState::A | TState::B => Some(TState::Top),
            TState::C => Some(TState::Other),
        }
    }

    fn is_leaf(self) -> bool {
        matches!(self, TState::A | TState::B | TState::C)
    }

    fn path(self) -> &'static [Self] {
        match self {
            TState::Top => &[TState::Top],
            TState::A => &[TState::Top, TState::A],
            TState::B => &[TState::Top, TState::B],
            TState::Other => &[TState::Other],
            TState::C => &[TState::Other, TState::C],
        }
    }

    fn as_index(self) -> usize {
        match self {
            TState::Top => 0,
            TState::A => 1,
            TState::B => 2,
            TState::Other => 3,
            TState::C => 4,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TEvent {
    Lifecycle(LifecycleCommand),
    GoB,
    GoC,
    Unhandled,
    UnhandledDeep,
    NoOp,
    SelfLoop,
    Reset,
    TriggerErr,
    Finish,
    Fail,
}

impl crate::event_tag::EventTag for TEvent {
    fn event_tag(&self) -> u8 {
        match self {
            TEvent::Lifecycle(_) => crate::event_tag::LIFECYCLE_TAG,
            TEvent::GoB => 0,
            TEvent::GoC => 1,
            TEvent::Unhandled => 2,
            TEvent::UnhandledDeep => 3,
            TEvent::NoOp => 4,
            TEvent::SelfLoop => 5,
            TEvent::Reset => 7,
            TEvent::TriggerErr => 8,
            TEvent::Finish => 9,
            TEvent::Fail => 10,
        }
    }
}

impl TEvent {
    pub const GO_B_TAG: u8 = 0;
    pub const GO_C_TAG: u8 = 1;
    pub const UNHANDLED_TAG: u8 = 2;
    pub const UNHANDLED_DEEP_TAG: u8 = 3;
    pub const NO_OP_TAG: u8 = 4;
    pub const SELF_LOOP_TAG: u8 = 5;
    pub const RESET_TAG: u8 = 7;
    pub const TRIGGER_ERR_TAG: u8 = 8;
    pub const FINISH_TAG: u8 = 9;
    pub const FAIL_TAG: u8 = 10;
}

impl LifecycleEvent for TEvent {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        match self {
            TEvent::Lifecycle(cmd) => Some(*cmd),
            _ => None,
        }
    }
}

pub struct TCtx;

pub struct TSpec;

impl MachineSpec for TSpec {
    type State = TState;
    type Event = TEvent;
    type Ctx = TCtx;
    type Mailboxes<R: crate::capability::BloxRuntime> = crate::mailboxes::NoMailboxes;

    const HANDLER_TABLE: &'static [&'static crate::spec::StateFns<Self>] =
        &[&TOP_FNS, &A_FNS, &B_FNS, &OTHER_FNS, &C_FNS];

    fn initial_state() -> TState {
        TState::A
    }

    fn on_init_entry(_ctx: &mut TCtx) {
        log("Init:entry");
    }

    fn on_init_exit(_ctx: &mut TCtx) {
        log("Init:exit");
    }

    fn root_transitions() -> &'static [StateRule<Self>] {
        &ROOT_RULES
    }
}

pub static ROOT_RULES: [StateRule<TSpec>; 3] = [
    StateRule {
        event_tag: TEvent::UNHANDLED_DEEP_TAG,
        matches: |ev| matches!(ev, TEvent::UnhandledDeep),
        actions: &[|_, _| {
            log("root_on_event:UnhandledDeep");
            ActionResult::Ok
        }],
        guard: |_, _, _| Decision::Stay,
    },
    StateRule {
        event_tag: TEvent::RESET_TAG,
        matches: |ev| matches!(ev, TEvent::Reset),
        actions: &[],
        guard: |_, _, _| Decision::Reset,
    },
    StateRule {
        event_tag: TEvent::FINISH_TAG,
        matches: |ev| matches!(ev, TEvent::Finish),
        actions: &[],
        guard: |_, _, _| Decision::Done,
    },
];

pub static TOP_FNS: StateFns<TSpec> = StateFns {
    on_entry: &[|_| log("Top:entry")],
    on_exit: &[|_| log("Top:exit")],
    transitions: &[StateRule {
        event_tag: TEvent::UNHANDLED_TAG,
        matches: |ev| matches!(ev, TEvent::Unhandled),
        actions: &[|_, _| {
            log("Top:handled_Unhandled");
            ActionResult::Ok
        }],
        guard: |_, _, _| Decision::Stay,
    }],
};

pub static A_FNS: StateFns<TSpec> = StateFns {
    on_entry: &[|_| log("A:entry")],
    on_exit: &[|_| log("A:exit")],
    transitions: &[
        StateRule {
            event_tag: TEvent::GO_B_TAG,
            matches: |ev| matches!(ev, TEvent::GoB),
            actions: &[],
            guard: |_, _, _| Decision::Transition(LeafState::new(TState::B)),
        },
        StateRule {
            event_tag: TEvent::GO_C_TAG,
            matches: |ev| matches!(ev, TEvent::GoC),
            actions: &[],
            guard: |_, _, _| Decision::Transition(LeafState::new(TState::C)),
        },
        StateRule {
            event_tag: TEvent::NO_OP_TAG,
            matches: |ev| matches!(ev, TEvent::NoOp),
            actions: &[],
            guard: |_, _, _| Decision::Stay,
        },
        StateRule {
            event_tag: TEvent::SELF_LOOP_TAG,
            matches: |ev| matches!(ev, TEvent::SelfLoop),
            actions: &[],
            guard: |_, _, _| Decision::Transition(LeafState::new(TState::A)),
        },
        StateRule {
            event_tag: TEvent::TRIGGER_ERR_TAG,
            matches: |ev| matches!(ev, TEvent::TriggerErr),
            actions: &[|_, _| {
                log("A:TriggerErr:action");
                ActionResult::Err
            }],
            guard: |_, results, _| {
                if results.any_failed() {
                    Decision::Transition(LeafState::new(TState::C))
                } else {
                    Decision::Stay
                }
            },
        },
        // Decision::Fail with the default error_state() (= None): the engine
        // goes to Init, firing the exit chain + on_init_entry.
        StateRule {
            event_tag: TEvent::FAIL_TAG,
            matches: |ev| matches!(ev, TEvent::Fail),
            actions: &[|_, _| {
                log("A:Fail:action");
                ActionResult::Ok
            }],
            guard: |_, _, _| Decision::Fail,
        },
    ],
};

pub static B_FNS: StateFns<TSpec> = StateFns {
    on_entry: &[|_| log("B:entry")],
    on_exit: &[|_| log("B:exit")],
    transitions: &[],
};

pub static OTHER_FNS: StateFns<TSpec> = StateFns {
    on_entry: &[|_| log("Other:entry")],
    on_exit: &[|_| log("Other:exit")],
    transitions: &[],
};

pub static C_FNS: StateFns<TSpec> = StateFns {
    on_entry: &[|_| log("C:entry")],
    on_exit: &[|_| log("C:exit")],
    transitions: &[],
};

pub fn machine_in_a() -> StateMachine<TSpec> {
    let mut m = StateMachine::<TSpec>::new(TCtx);
    m.dispatch(TEvent::Lifecycle(LifecycleCommand::Start));
    take_log();
    m
}

pub fn machine_in_c() -> StateMachine<TSpec> {
    let mut m = machine_in_a();
    m.dispatch(TEvent::GoC);
    take_log();
    m
}

// ── Error-state fixture ─────────────────────────────────────────────────────
//
// A second spec with an `error_state()`/`is_error` override: `Err` is a leaf
// error state (child of `Top`, sibling of initial state `A`). `Decision::Fail`
// transitions into it; while there, the state is absorbing — domain events are
// dropped without evaluating state rules or root rules (engine.rs). Log
// strings are "E"-prefixed to stay distinct from the TSpec log.

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub enum EState {
    Top,
    #[default]
    A,
    Err,
}

impl crate::topology::StateTopology for EState {
    const STATE_COUNT: usize = 3;

    fn parent(self) -> Option<Self> {
        match self {
            EState::Top => None,
            EState::A | EState::Err => Some(EState::Top),
        }
    }

    fn is_leaf(self) -> bool {
        matches!(self, EState::A | EState::Err)
    }

    fn path(self) -> &'static [Self] {
        match self {
            EState::Top => &[EState::Top],
            EState::A => &[EState::Top, EState::A],
            EState::Err => &[EState::Top, EState::Err],
        }
    }

    fn as_index(self) -> usize {
        match self {
            EState::Top => 0,
            EState::A => 1,
            EState::Err => 2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EEvent {
    Lifecycle(LifecycleCommand),
    Boom,
}

impl crate::event_tag::EventTag for EEvent {
    fn event_tag(&self) -> u8 {
        match self {
            EEvent::Lifecycle(_) => crate::event_tag::LIFECYCLE_TAG,
            EEvent::Boom => 0,
        }
    }
}

impl EEvent {
    pub const BOOM_TAG: u8 = 0;
}

impl LifecycleEvent for EEvent {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        match self {
            EEvent::Lifecycle(cmd) => Some(*cmd),
            _ => None,
        }
    }
}

pub struct ECtx;

pub struct ESpec;

impl MachineSpec for ESpec {
    type State = EState;
    type Event = EEvent;
    type Ctx = ECtx;
    type Mailboxes<R: crate::capability::BloxRuntime> = crate::mailboxes::NoMailboxes;

    const HANDLER_TABLE: &'static [&'static crate::spec::StateFns<Self>] =
        &[&E_TOP_FNS, &E_A_FNS, &E_ERR_FNS];

    fn initial_state() -> EState {
        EState::A
    }

    fn error_state() -> Option<EState> {
        Some(EState::Err)
    }

    fn is_error(state: &EState) -> bool {
        matches!(state, EState::Err)
    }

    fn on_init_entry(_ctx: &mut ECtx) {
        log("EInit:entry");
    }

    fn on_init_exit(_ctx: &mut ECtx) {
        log("EInit:exit");
    }

    fn root_transitions() -> &'static [StateRule<Self>] {
        &E_ROOT_RULES
    }
}

pub static E_ROOT_RULES: [StateRule<ESpec>; 1] = [StateRule {
    event_tag: EEvent::BOOM_TAG,
    matches: |ev| matches!(ev, EEvent::Boom),
    actions: &[|_, _| {
        log("Eroot:Boom:action");
        ActionResult::Ok
    }],
    guard: |_, _, _| Decision::Stay,
}];

pub static E_TOP_FNS: StateFns<ESpec> = StateFns {
    on_entry: &[|_| log("ETop:entry")],
    on_exit: &[|_| log("ETop:exit")],
    transitions: &[],
};

pub static E_A_FNS: StateFns<ESpec> = StateFns {
    on_entry: &[|_| log("EA:entry")],
    on_exit: &[|_| log("EA:exit")],
    transitions: &[StateRule {
        event_tag: EEvent::BOOM_TAG,
        matches: |ev| matches!(ev, EEvent::Boom),
        actions: &[|_, _| {
            log("EA:Boom:action");
            ActionResult::Ok
        }],
        guard: |_, _, _| Decision::Fail,
    }],
};

pub static E_ERR_FNS: StateFns<ESpec> = StateFns {
    on_entry: &[|_| log("EErr:entry")],
    on_exit: &[|_| log("EErr:exit")],
    // This rule must NEVER fire: error states are absorbing, so domain events
    // are dropped before any state rule (or root rule) is evaluated.
    transitions: &[StateRule {
        event_tag: EEvent::BOOM_TAG,
        matches: |ev| matches!(ev, EEvent::Boom),
        actions: &[|_, _| {
            log("EErr:Boom:action");
            ActionResult::Ok
        }],
        guard: |_, _, _| Decision::Transition(LeafState::new(EState::A)),
    }],
};

/// Spec whose `error_state()` returns a composite state — `StateMachine::new`
/// debug_asserts that an error state must be a leaf.
pub struct BadESpec;

impl MachineSpec for BadESpec {
    type State = EState;
    type Event = EEvent;
    type Ctx = ECtx;
    type Mailboxes<R: crate::capability::BloxRuntime> = crate::mailboxes::NoMailboxes;

    const HANDLER_TABLE: &'static [&'static crate::spec::StateFns<Self>] =
        &[&BAD_E_FNS, &BAD_E_FNS, &BAD_E_FNS];

    fn initial_state() -> EState {
        EState::A
    }

    fn error_state() -> Option<EState> {
        Some(EState::Top) // composite — must panic in debug builds
    }

    fn is_error(state: &EState) -> bool {
        matches!(state, EState::Top)
    }
}

pub static BAD_E_FNS: StateFns<BadESpec> = StateFns {
    on_entry: &[],
    on_exit: &[],
    transitions: &[],
};

/// Spec whose `HANDLER_TABLE` length disagrees with `State::STATE_COUNT`
/// (2 entries vs 3 states) — `StateMachine::new` asserts the match in every
/// build profile, since hand-written specs get no codegen guarantee.
pub struct BadTableSpec;

impl MachineSpec for BadTableSpec {
    type State = EState;
    type Event = EEvent;
    type Ctx = ECtx;
    type Mailboxes<R: crate::capability::BloxRuntime> = crate::mailboxes::NoMailboxes;

    const HANDLER_TABLE: &'static [&'static crate::spec::StateFns<Self>] =
        &[&BAD_TABLE_FNS, &BAD_TABLE_FNS];

    fn initial_state() -> EState {
        EState::A
    }
}

pub static BAD_TABLE_FNS: StateFns<BadTableSpec> = StateFns {
    on_entry: &[],
    on_exit: &[],
    transitions: &[],
};

pub fn machine_in_ea() -> StateMachine<ESpec> {
    let mut m = StateMachine::<ESpec>::new(ECtx);
    m.dispatch(EEvent::Lifecycle(LifecycleCommand::Start));
    take_log();
    m
}

/// Spec whose `initial_state()` IS the error state — degenerate but legal.
/// Exercises the engine normalization: `Started(error)` must become `Failed`.
pub struct InitErrSpec;

impl MachineSpec for InitErrSpec {
    type State = EState;
    type Event = EEvent;
    type Ctx = ECtx;
    type Mailboxes<R: crate::capability::BloxRuntime> = crate::mailboxes::NoMailboxes;

    const HANDLER_TABLE: &'static [&'static crate::spec::StateFns<Self>] =
        &[&INIT_ERR_FNS, &INIT_ERR_FNS, &INIT_ERR_FNS];

    fn initial_state() -> EState {
        EState::Err
    }

    fn error_state() -> Option<EState> {
        Some(EState::Err)
    }

    fn is_error(state: &EState) -> bool {
        matches!(state, EState::Err)
    }
}

pub static INIT_ERR_FNS: StateFns<InitErrSpec> = StateFns {
    on_entry: &[],
    on_exit: &[],
    transitions: &[],
};
