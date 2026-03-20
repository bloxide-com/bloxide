// Copyright 2025 Bloxide, all rights reserved
//! Tests for lifecycle dispatch through the engine.
//!
//! These tests verify that lifecycle commands trigger proper state transitions
//! and callbacks when called directly via `handle_lifecycle()`, not just synthetic outcomes.

#[cfg(all(test, feature = "std"))]
mod tests {
    use crate::engine::{DispatchOutcome, MachineState, StateMachine};
    use crate::event_tag::{EventTag, LifecycleEvent};
    use crate::lifecycle::LifecycleCommand;
    use crate::messaging::Envelope;
    use crate::spec::MachineSpec;
    use crate::test_utils::TestRuntime;
    use crate::topology::{LeafState, StateTopology};
    use crate::transition::{ActionFn, ActionResults, Guard, TransitionRule};
    use core::marker::PhantomData;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    // ── Test Spy State Machine ─────────────────────────────────────────────

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
    enum TestState {
        #[default]
        Init, // Will be tracked by MachineState::Init
        Running,
        Done,
    }

    impl StateTopology for TestState {
        const STATE_COUNT: usize = 3;
        fn parent(self) -> Option<Self> {
            None
        }
        fn is_leaf(self) -> bool {
            true
        }
        fn path(self) -> &'static [Self] {
            match self {
                TestState::Init => &[TestState::Init],
                TestState::Running => &[TestState::Running],
                TestState::Done => &[TestState::Done],
            }
        }
        fn as_index(self) -> usize {
            self as usize
        }
    }

    /// Test event type with lifecycle, message, and completion variants.
    ///
    /// - `Lifecycle` wraps lifecycle commands for dispatch testing
    /// - `Msg` satisfies the `From<Envelope<u32>>` requirement for TestRuntime mailboxes
    /// - `Complete` triggers transition to terminal Done state
    #[derive(Debug, Clone, Copy)]
    enum TestEvent {
        Lifecycle(LifecycleCommand),
        // Field exists to match Envelope<u32> payload; value is not examined in tests
        #[allow(dead_code)]
        Msg(u32),
        Complete,
    }

    impl EventTag for TestEvent {
        fn event_tag(&self) -> u8 {
            match self {
                TestEvent::Lifecycle(_) => 254, // LIFECYCLE_TAG
                TestEvent::Msg(_) => 0,
                TestEvent::Complete => 1,
            }
        }
    }

    impl LifecycleEvent for TestEvent {
        fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
            match self {
                TestEvent::Lifecycle(cmd) => Some(*cmd),
                _ => None,
            }
        }
    }

    impl From<Envelope<u32>> for TestEvent {
        fn from(env: Envelope<u32>) -> Self {
            TestEvent::Msg(env.1)
        }
    }

    /// Spy context that tracks which callbacks fire using atomic counters.
    #[derive(Default)]
    struct SpyCtx {
        running_entry_count: Arc<AtomicU32>,
        running_exit_count: Arc<AtomicU32>,
        done_entry_count: Arc<AtomicU32>,
        init_entry_count: Arc<AtomicU32>,
    }

    struct TestSpec<R>(PhantomData<R>);

    // Static state functions for the handler table
    fn running_entry(ctx: &mut SpyCtx) {
        ctx.running_entry_count.fetch_add(1, Ordering::SeqCst);
    }

    fn running_exit(ctx: &mut SpyCtx) {
        ctx.running_exit_count.fetch_add(1, Ordering::SeqCst);
    }

    fn done_entry(ctx: &mut SpyCtx) {
        ctx.done_entry_count.fetch_add(1, Ordering::SeqCst);
    }

    fn init_entry(ctx: &mut SpyCtx) {
        ctx.init_entry_count.fetch_add(1, Ordering::SeqCst);
    }

    impl<R: crate::capability::BloxRuntime> MachineSpec for TestSpec<R> {
        type State = TestState;
        type Event = TestEvent;
        type Ctx = SpyCtx;
        type Mailboxes<Rt: crate::capability::BloxRuntime> = (Rt::Stream<u32>,);

        const HANDLER_TABLE: &'static [&'static crate::spec::StateFns<Self>] = &[
            // Init - empty, handled by engine
            &crate::spec::StateFns {
                on_entry: &[],
                on_exit: &[],
                transitions: &[],
            },
            // Running - with transition to Done on Complete event
            &crate::spec::StateFns {
                on_entry: &[running_entry],
                on_exit: &[running_exit],
                transitions: &[TransitionRule {
                    event_tag: 1, // Complete tag
                    matches: |event: &TestEvent| matches!(event, TestEvent::Complete),
                    actions: &[] as &[ActionFn<Self>],
                    guard: |_ctx: &SpyCtx, _results: &ActionResults, _event: &TestEvent| {
                        Guard::Transition(LeafState::new(TestState::Done))
                    },
                }],
            },
            // Done (terminal state)
            &crate::spec::StateFns {
                on_entry: &[done_entry],
                on_exit: &[],
                transitions: &[],
            },
        ];

        fn initial_state() -> TestState {
            TestState::Running
        }
        fn is_terminal(state: &TestState) -> bool {
            matches!(state, TestState::Done)
        }
        fn is_error(_state: &TestState) -> bool {
            false
        }

        fn on_init_entry(ctx: &mut SpyCtx) {
            init_entry(ctx);
        }
    }

    // ── Test Cases ─────────────────────────────────────────────────────────

    #[test]
    fn start_from_init_fires_on_entry() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);

        // Machine starts in Init
        assert!(machine.current_state().is_init());

        // Dispatch Start via handle_lifecycle
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);

        // Verify outcome
        assert!(matches!(
            outcome,
            DispatchOutcome::Started(MachineState::State(TestState::Running))
        ));

        // Verify state changed
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));

        // CRITICAL: Verify on_entry fired
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);

        // Verify NO on_exit (no transition from operational state)
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn start_from_operational_is_noop() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);

        // Start first
        machine.handle_lifecycle(LifecycleCommand::Start);

        // Dispatch Start again
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);

        // Verify outcome shows no transition
        assert!(matches!(outcome, DispatchOutcome::HandledNoTransition));

        // Verify state unchanged
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));

        // CRITICAL: Verify NO additional callbacks fired (entry count stays at 1)
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn reset_fires_exit_chain_and_on_init_entry() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);

        // Start and verify we're in Running
        machine.handle_lifecycle(LifecycleCommand::Start);
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));

        // Dispatch Reset
        let outcome = machine.handle_lifecycle(LifecycleCommand::Reset);

        // Verify outcome
        assert!(matches!(outcome, DispatchOutcome::Reset));

        // Verify state changed back to Init
        assert!(machine.current_state().is_init());

        // CRITICAL: Verify on_exit fired for Running
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);

        // CRITICAL: Verify on_init_entry fired
        assert_eq!(machine.ctx().init_entry_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stop_fires_exit_chain_and_reports_stopped() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);

        // Start
        machine.handle_lifecycle(LifecycleCommand::Start);

        // Dispatch Stop
        let outcome = machine.handle_lifecycle(LifecycleCommand::Stop);

        // Verify outcome
        assert!(matches!(outcome, DispatchOutcome::Stopped));

        // Verify state changed to Init
        assert!(machine.current_state().is_init());

        // CRITICAL: Verify on_exit fired
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);

        // CRITICAL: Verify on_init_entry fired
        assert_eq!(machine.ctx().init_entry_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn ping_returns_alive_without_state_change() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);

        // Start
        machine.handle_lifecycle(LifecycleCommand::Start);

        // Dispatch Ping
        let outcome = machine.handle_lifecycle(LifecycleCommand::Ping);

        // Verify outcome
        assert!(matches!(outcome, DispatchOutcome::Alive));

        // Verify state unchanged
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));

        // Verify NO additional callbacks fired
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1); // Only from initial Start
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn transition_to_terminal_state_fires_on_entry_and_reports_done() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);

        // Start
        machine.handle_lifecycle(LifecycleCommand::Start);
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);

        // Dispatch Complete event to transition to Done (terminal state)
        let outcome = machine.dispatch(TestEvent::Complete);

        // Verify outcome is Done
        assert!(matches!(
            outcome,
            DispatchOutcome::Done(MachineState::State(TestState::Done))
        ));

        // Verify state changed to Done
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Done)
        ));

        // CRITICAL: Verify on_exit fired for Running
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);

        // CRITICAL: Verify on_entry fired for Done
        assert_eq!(machine.ctx().done_entry_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn dispatch_with_lifecycle_event_variant_works() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);

        // Dispatch Start via dispatch() with Lifecycle event variant
        let outcome = machine.dispatch(TestEvent::Lifecycle(LifecycleCommand::Start));

        // Verify outcome
        assert!(matches!(
            outcome,
            DispatchOutcome::Started(MachineState::State(TestState::Running))
        ));

        // Verify state changed
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));
    }
}
