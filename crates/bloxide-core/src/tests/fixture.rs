/// Shared HSM fixture for engine unit tests.
///
/// Provides a minimal but complete `MachineSpec` implementation (`TSpec`) used
/// by all engine tests. The topology exercises every structural case the engine
/// must handle:
///
/// ```text
/// [VirtualRoot — engine implicit]
/// ├── Top       (composite, top-level)
/// │   ├── A     (leaf)
/// │   └── B     (leaf)
/// └── Other     (composite, top-level)
///     └── C     (leaf)
/// ```
///
/// `initial_state()` = `A` (first operational leaf after `Start`).
///
/// This fixture is the canonical example of how to build a `TestRuntime`-based
/// test harness. Blox-level tests follow the same pattern — see
/// `skills/spec-driven-development/SKILL.md` and `spec/bloxes/<name>.md`.
use crate::engine::StateMachine;
use crate::spec::{MachineSpec, StateFns};
use crate::topology::LeafState;
use crate::transition::{ActionResult, Guard, StateRule};
use std::cell::RefCell;
use std::thread_local;
use std::vec::Vec;

// ── Shared event log ──────────────────────────────────────────────────────────
//
// Thread-local sink for on_entry / on_exit callbacks. Tests call `log(msg)`
// inside handlers and `take_log()` to assert the firing sequence.
// Using a thread-local (rather than storing in Ctx) keeps `TCtx` trivial
// and makes the log independent of the machine's borrow.

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

// ── States ────────────────────────────────────────────────────────────────────
//
// Root and Init are engine-implicit — never declared in the user's State enum.
// TState implements StateTopology manually so we can test the engine independently
// of proc-macro infrastructure.

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum TState {
    Top,
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

// ── Events ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum TEvent {
    GoB,
    GoC,
    Unhandled,
    UnhandledDeep,
    NoOp,
    SelfLoop,
    /// Satisfied by `is_start` — transitions out of Init into `initial_state()` (A).
    Start,
    /// Triggers `Guard::Reset` from root rules.
    Reset,
    /// Triggers an action that returns `ActionResult::Err`, used to test the error guard path.
    TriggerErr,
}

impl crate::event_tag::EventTag for TEvent {
    fn event_tag(&self) -> u8 {
        match self {
            TEvent::GoB => 0,
            TEvent::GoC => 1,
            TEvent::Unhandled => 2,
            TEvent::UnhandledDeep => 3,
            TEvent::NoOp => 4,
            TEvent::SelfLoop => 5,
            TEvent::Start => 6,
            TEvent::Reset => 7,
            TEvent::TriggerErr => 8,
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
}

// ── Context ───────────────────────────────────────────────────────────────────

pub struct TCtx;

// ── Spec ──────────────────────────────────────────────────────────────────────

pub struct TSpec;

impl MachineSpec for TSpec {
    type State = TState;
    type Event = TEvent;
    type Ctx = TCtx;
    type Mailboxes<R: crate::capability::BloxRuntime> = crate::mailboxes::NoMailboxes;

    // HANDLER_TABLE uses TState index order (Top=0, A=1, B=2, Other=3, C=4)
    const HANDLER_TABLE: &'static [&'static crate::spec::StateFns<Self>] = &[
        &TOP_FNS,   // index 0 = Top
        &A_FNS,     // index 1 = A
        &B_FNS,     // index 2 = B
        &OTHER_FNS, // index 3 = Other
        &C_FNS,     // index 4 = C
    ];

    fn initial_state() -> TState {
        TState::A
    }

    fn on_init_entry(_ctx: &mut TCtx) {
        log("Init:entry");
    }

    fn on_init_exit(_ctx: &mut TCtx) {
        log("Init:exit");
    }

    fn is_start(event: &TEvent) -> bool {
        matches!(event, TEvent::Start)
    }

    fn root_transitions() -> &'static [StateRule<TSpec>] {
        &ROOT_RULES
    }
}

// ── Root rules ────────────────────────────────────────────────────────────────

pub static ROOT_RULES: [StateRule<TSpec>; 2] = [
    StateRule {
        event_tag: TEvent::UNHANDLED_DEEP_TAG,
        matches: |ev| matches!(ev, TEvent::UnhandledDeep),
        actions: &[|_, _| {
            log("root_on_event:UnhandledDeep");
            ActionResult::Ok
        }],
        guard: |_, _, _| Guard::Stay,
    },
    StateRule {
        event_tag: TEvent::RESET_TAG,
        matches: |ev| matches!(ev, TEvent::Reset),
        actions: &[],
        guard: |_, _, _| Guard::Reset,
    },
];

// ── State function tables ─────────────────────────────────────────────────────

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
        guard: |_, _, _| Guard::Stay,
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
            guard: |_, _, _| Guard::Transition(LeafState::new(TState::B)),
        },
        StateRule {
            event_tag: TEvent::GO_C_TAG,
            matches: |ev| matches!(ev, TEvent::GoC),
            actions: &[],
            guard: |_, _, _| Guard::Transition(LeafState::new(TState::C)),
        },
        StateRule {
            event_tag: TEvent::NO_OP_TAG,
            matches: |ev| matches!(ev, TEvent::NoOp),
            actions: &[],
            guard: |_, _, _| Guard::Stay,
        },
        StateRule {
            event_tag: TEvent::SELF_LOOP_TAG,
            matches: |ev| matches!(ev, TEvent::SelfLoop),
            actions: &[],
            guard: |_, _, _| Guard::Transition(LeafState::new(TState::A)),
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
                    Guard::Transition(LeafState::new(TState::C))
                } else {
                    Guard::Stay
                }
            },
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Create a machine that is in operational state A (post-Start).
pub fn machine_in_a() -> StateMachine<TSpec> {
    let mut m = StateMachine::<TSpec>::new(TCtx);
    m.dispatch(TEvent::Start);
    take_log(); // discard "Init:exit", "Top:entry", "A:entry"
    m
}

/// Create a machine that is in operational state C (under Other).
pub fn machine_in_c() -> StateMachine<TSpec> {
    let mut m = machine_in_a();
    m.dispatch(TEvent::GoC);
    take_log(); // discard transition logs
    m
}
