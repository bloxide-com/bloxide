// Copyright 2025 Bloxide, all rights reserved
//! Integration test for or-patterns in `transitions!` (GitHub issue #58).
use bloxide_core::engine::{DispatchOutcome, MachineState, StateMachine};
use bloxide_core::event_tag::{EventTag, LifecycleEvent, WILDCARD_TAG};
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_core::mailboxes::NoMailboxes;
use bloxide_core::spec::{MachineSpec, StateFns};
use bloxide_core::topology::StateTopology;
use bloxide_core::transition::ActionResult;
use bloxide_macros::transitions;
use core::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
enum OrState { #[default] Init, Idle, Active, Done }

impl StateTopology for OrState {
    const STATE_COUNT: usize = 4;
    fn parent(self) -> Option<Self> { None }
    fn is_leaf(self) -> bool { true }
    fn path(self) -> &'static [Self] {
        match self {
            OrState::Init => &[OrState::Init],
            OrState::Idle => &[OrState::Idle],
            OrState::Active => &[OrState::Active],
            OrState::Done => &[OrState::Done],
        }
    }
    fn as_index(self) -> usize { self as usize }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum OrEvent {
    Lifecycle(LifecycleCommand),
    GoActive,
    GoDone,
    Tick,
}

impl EventTag for OrEvent {
    fn event_tag(&self) -> u8 {
        match self {
            OrEvent::Lifecycle(_) => 254,
            OrEvent::GoActive => 0,
            OrEvent::GoDone => 1,
            OrEvent::Tick => 2,
        }
    }
}

#[allow(dead_code)]
impl OrEvent {
    pub const GO_ACTIVE_TAG: u8 = 0;
    pub const GO_DONE_TAG: u8 = 1;
    pub const TICK_TAG: u8 = 2;
}

impl LifecycleEvent for OrEvent {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        match self {
            OrEvent::Lifecycle(cmd) => Some(*cmd),
            _ => None,
        }
    }
}

struct OrCtx {
    or_action_count: Arc<AtomicU32>,
    last_event_tag: Arc<AtomicU32>,
    tick_action_count: Arc<AtomicU32>,
}

fn or_action(ctx: &mut OrCtx, event: &OrEvent) -> ActionResult {
    ctx.or_action_count.fetch_add(1, Ordering::SeqCst);
    ctx.last_event_tag.store(event.event_tag() as u32, Ordering::SeqCst);
    ActionResult::Ok
}

fn tick_action(ctx: &mut OrCtx, _event: &OrEvent) -> ActionResult {
    ctx.tick_action_count.fetch_add(1, Ordering::SeqCst);
    ActionResult::Ok
}

type TR = bloxide_core::test_utils::TestRuntime;

struct OrSpec<R>(PhantomData<R>);

impl<R: bloxide_core::capability::BloxRuntime> MachineSpec for OrSpec<R> {
    type State = OrState;
    type Event = OrEvent;
    type Ctx = OrCtx;
    type Mailboxes<Rt: bloxide_core::capability::BloxRuntime> = NoMailboxes;

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = &[
        &StateFns { on_entry: &[], on_exit: &[], transitions: &[] },
        &StateFns {
            on_entry: &[],
            on_exit: &[],
            transitions: transitions![
                OrEvent::GoActive | OrEvent::GoDone => {
                    actions [or_action]
                    transition OrState::Active
                },
                OrEvent::Tick => {
                    actions [tick_action]
                    stay
                },
            ],
        },
        &StateFns { on_entry: &[], on_exit: &[], transitions: &[] },
        &StateFns { on_entry: &[], on_exit: &[], transitions: &[] },
    ];

    fn initial_state() -> OrState { OrState::Idle }
}

fn make_machine() -> (StateMachine<OrSpec<TR>>, Arc<AtomicU32>, Arc<AtomicU32>, Arc<AtomicU32>) {
    let or_count = Arc::new(AtomicU32::new(0));
    let last_tag = Arc::new(AtomicU32::new(99));
    let tick_count = Arc::new(AtomicU32::new(0));
    let ctx = OrCtx {
        or_action_count: or_count.clone(),
        last_event_tag: last_tag.clone(),
        tick_action_count: tick_count.clone(),
    };
    let mut m = StateMachine::<OrSpec<TR>>::new(ctx);
    m.handle_lifecycle(LifecycleCommand::Start);
    assert!(matches!(m.current_state(), MachineState::State(OrState::Idle)));
    (m, or_count, last_tag, tick_count)
}

/// The or-pattern rule must use WILDCARD_TAG, not just the first variant's tag.
#[test]
fn or_pattern_rule_uses_wildcard_tag() {
    let idle_fns = &OrSpec::<TR>::HANDLER_TABLE[1];
    // The or-pattern rule is the first rule (index 0).
    let or_rule = &idle_fns.transitions[0];
    assert_eq!(
        or_rule.event_tag, WILDCARD_TAG,
        "or-pattern rule must use WILDCARD_TAG so both variants pass the fast pre-filter"
    );
}

/// First variant in the or-pattern (GoActive, tag=0) triggers the action.
#[test]
fn or_pattern_matches_first_variant() {
    let (mut m, or_count, last_tag, _tick_count) = make_machine();
    let outcome = m.dispatch(OrEvent::GoActive);
    assert!(matches!(outcome, DispatchOutcome::Transition(MachineState::State(OrState::Active))));
    assert_eq!(or_count.load(Ordering::SeqCst), 1);
    assert_eq!(last_tag.load(Ordering::SeqCst), 0);
}

/// Second variant in the or-pattern (GoDone, tag=1) must also trigger the
/// action. Before the fix, the fast pre-filter used only tag=0 and silently
/// rejected tag=1 events.
#[test]
fn or_pattern_matches_second_variant() {
    let (mut m, or_count, last_tag, _tick_count) = make_machine();
    let outcome = m.dispatch(OrEvent::GoDone);
    assert!(matches!(outcome, DispatchOutcome::Transition(MachineState::State(OrState::Active))));
    assert_eq!(or_count.load(Ordering::SeqCst), 1);
    assert_eq!(last_tag.load(Ordering::SeqCst), 1);
}

/// A single-variant rule (Tick) after the or-pattern should still work.
#[test]
fn single_variant_rule_after_or_pattern_still_works() {
    let (mut m, _or_count, _last_tag, tick_count) = make_machine();
    let outcome = m.dispatch(OrEvent::Tick);
    assert!(matches!(outcome, DispatchOutcome::HandledNoTransition));
    assert_eq!(tick_count.load(Ordering::SeqCst), 1);
    assert!(matches!(m.current_state(), MachineState::State(OrState::Idle)));
}
