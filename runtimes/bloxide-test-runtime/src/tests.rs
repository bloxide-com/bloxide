// Copyright 2025 Bloxide, all rights reserved

// ── Spawn helper tests ─────────────────────────────────────────────────────
//
// `spawn_dynamic_child` (bloxide-spawn) tests live here rather than in bloxide-spawn:
// a bloxide-spawn dev-dependency on this crate would be a dev-dependency
// cycle (two non-unifying `bloxide-spawn` instances in the graph).

mod spawn_helper_tests;

// ── Waker tests ──────────────────────────────────────────────────────────

mod waker_tests {
    use bloxide_core::capability::{BloxRuntime, DynamicChannelCap};
    use bloxide_core::engine::StateMachine;
    use bloxide_core::event_tag::{EventTag, LifecycleEvent};
    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::messaging::Envelope;
    use bloxide_core::spec::MachineSpec;
    use bloxide_core::topology::StateTopology;
    use bloxide_core::transition::{ActionResult, Decision, TransitionRule};
    use bloxide_core::{run, RunConfig};
    use std::marker::PhantomData;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use crate::TestRuntime;

    /// Minimal block_on that only re-polls when woken.
    fn block_on<F: core::future::Future>(future: F) -> F::Output {
        struct WakeFlag {
            woken: AtomicBool,
        }

        fn make_raw_waker(flag: *const WakeFlag) -> RawWaker {
            unsafe fn clone(flag: *const ()) -> RawWaker {
                make_raw_waker(flag as *const WakeFlag)
            }
            unsafe fn wake(flag: *const ()) {
                (*(flag as *const WakeFlag))
                    .woken
                    .store(true, Ordering::SeqCst);
            }
            unsafe fn wake_by_ref(flag: *const ()) {
                (*(flag as *const WakeFlag))
                    .woken
                    .store(true, Ordering::SeqCst);
            }
            unsafe fn drop_waker(_flag: *const ()) {}
            static VTABLE: RawWakerVTable =
                RawWakerVTable::new(clone, wake, wake_by_ref, drop_waker);
            RawWaker::new(flag as *const (), &VTABLE)
        }

        let flag = Arc::new(WakeFlag {
            woken: AtomicBool::new(true),
        });
        let raw = make_raw_waker(Arc::as_ptr(&flag));
        let waker = unsafe { Waker::from_raw(raw) };
        let mut cx = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => {
                    while !flag.woken.swap(false, Ordering::SeqCst) {
                        std::thread::yield_now();
                    }
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
    enum WState {
        #[default]
        Init,
        Running,
    }

    impl StateTopology for WState {
        const STATE_COUNT: usize = 2;
        fn parent(self) -> Option<Self> {
            None
        }
        fn is_leaf(self) -> bool {
            true
        }
        fn path(self) -> &'static [Self] {
            match self {
                WState::Init => &[WState::Init],
                WState::Running => &[WState::Running],
            }
        }
        fn as_index(self) -> usize {
            self as usize
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum WEvent {
        Lifecycle(LifecycleCommand),
        Msg(u32),
    }

    impl EventTag for WEvent {
        fn event_tag(&self) -> u8 {
            match self {
                WEvent::Lifecycle(_) => 254,
                WEvent::Msg(_) => 0,
            }
        }
    }

    impl LifecycleEvent for WEvent {
        fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
            match self {
                WEvent::Lifecycle(cmd) => Some(*cmd),
                _ => None,
            }
        }
    }

    impl From<Envelope<u32>> for WEvent {
        fn from(env: Envelope<u32>) -> Self {
            WEvent::Msg(env.1)
        }
    }

    struct WCtx {
        processed: Arc<AtomicU32>,
        threshold: u32,
    }

    struct WSpec<R>(PhantomData<R>);

    impl<R: bloxide_core::capability::BloxRuntime> MachineSpec for WSpec<R> {
        type State = WState;
        type Event = WEvent;
        type Ctx = WCtx;
        type Mailboxes<Rt: bloxide_core::capability::BloxRuntime> = (Rt::Stream<u32>,);

        const HANDLER_TABLE: &'static [&'static bloxide_core::spec::StateFns<Self>] = &[
            &bloxide_core::spec::StateFns {
                on_entry: &[],
                on_exit: &[],
                transitions: &[],
            },
            &bloxide_core::spec::StateFns {
                on_entry: &[],
                on_exit: &[],
                transitions: &[TransitionRule {
                    event_tag: 0,
                    matches: |ev| matches!(ev, WEvent::Msg(_payload)),
                    actions: &[|ctx, _ev| {
                        ctx.processed.fetch_add(1, Ordering::SeqCst);
                        ActionResult::Ok
                    }],
                    guard: |ctx, _results, _ev| {
                        if ctx.processed.load(Ordering::SeqCst) >= ctx.threshold {
                            Decision::Stop
                        } else {
                            Decision::Stay
                        }
                    },
                }],
            },
        ];

        fn initial_state() -> WState {
            WState::Running
        }
    }

    #[test]
    fn runtime_async_wakeup() {
        let id = TestRuntime::alloc_actor_id();
        let (sender_ref, receiver) = TestRuntime::channel::<u32>(id, 16);
        let stream = TestRuntime::to_stream(receiver);

        let processed = Arc::new(AtomicU32::new(0));
        let ctx = WCtx {
            processed: processed.clone(),
            threshold: 1,
        };
        let mut machine = StateMachine::<WSpec<TestRuntime>>::new(ctx);
        machine.dispatch(WEvent::Lifecycle(LifecycleCommand::Start));

        let sender_clone = sender_ref.clone();
        let handle = std::thread::spawn(move || {
            sender_clone.try_send(0, 42u32).unwrap();
        });

        block_on(run(machine, (stream,), RunConfig::<TestRuntime>::bare(), 0));

        handle.join().unwrap();
        assert_eq!(processed.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn runtime_waker_idempotent() {
        let id = TestRuntime::alloc_actor_id();
        let (sender_ref, receiver) = TestRuntime::channel::<u32>(id, 16);
        let stream = TestRuntime::to_stream(receiver);

        let processed = Arc::new(AtomicU32::new(0));
        let ctx = WCtx {
            processed: processed.clone(),
            threshold: 5,
        };
        let mut machine = StateMachine::<WSpec<TestRuntime>>::new(ctx);
        machine.dispatch(WEvent::Lifecycle(LifecycleCommand::Start));

        let sender_clone = sender_ref.clone();
        let handle = std::thread::spawn(move || {
            for i in 0..5u32 {
                sender_clone.try_send(0, i).unwrap();
            }
        });

        block_on(run(machine, (stream,), RunConfig::<TestRuntime>::bare(), 0));

        handle.join().unwrap();
        assert_eq!(processed.load(Ordering::SeqCst), 5);
    }
}

// ── Lifecycle dispatch tests ──────────────────────────────────────────────
//
// Moved from bloxide-core to avoid circular dev-dependency.
// These tests verify lifecycle command dispatch through the engine using
// TestRuntime as the concrete runtime.

mod lifecycle_dispatch {
    use bloxide_core::engine::{DispatchOutcome, MachineState, StateMachine};
    use bloxide_core::event_tag::{EventTag, LifecycleEvent};
    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::messaging::Envelope;
    use bloxide_core::spec::MachineSpec;
    use bloxide_core::topology::{LeafState, StateTopology};
    use bloxide_core::transition::{ActionFn, ActionResults, Decision, TransitionRule};
    use core::marker::PhantomData;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use crate::TestRuntime;

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
    enum TestState {
        #[default]
        Init,
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

    #[derive(Debug, Clone, Copy)]
    enum TestEvent {
        Lifecycle(LifecycleCommand),
        Msg(u32),
        Complete,
        GoRunning,
    }

    impl EventTag for TestEvent {
        fn event_tag(&self) -> u8 {
            match self {
                TestEvent::Lifecycle(_) => 254,
                TestEvent::Msg(_payload) => 0,
                TestEvent::Complete => 1,
                TestEvent::GoRunning => 2,
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

    #[derive(Default)]
    struct SpyCtx {
        running_entry_count: Arc<AtomicU32>,
        running_exit_count: Arc<AtomicU32>,
        done_entry_count: Arc<AtomicU32>,
        init_entry_count: Arc<AtomicU32>,
    }

    struct TestSpec<R>(PhantomData<R>);

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

    impl<R: bloxide_core::capability::BloxRuntime> MachineSpec for TestSpec<R> {
        type State = TestState;
        type Event = TestEvent;
        type Ctx = SpyCtx;
        type Mailboxes<Rt: bloxide_core::capability::BloxRuntime> = (Rt::Stream<u32>,);

        const HANDLER_TABLE: &'static [&'static bloxide_core::spec::StateFns<Self>] = &[
            &bloxide_core::spec::StateFns {
                on_entry: &[],
                on_exit: &[],
                transitions: &[],
            },
            &bloxide_core::spec::StateFns {
                on_entry: &[running_entry],
                on_exit: &[running_exit],
                transitions: &[TransitionRule {
                    event_tag: 1,
                    matches: |event: &TestEvent| matches!(event, TestEvent::Complete),
                    actions: &[] as &[ActionFn<Self>],
                    guard: |_ctx: &SpyCtx, _results: &ActionResults, _event: &TestEvent| {
                        Decision::Transition(LeafState::new(TestState::Done))
                    },
                }],
            },
            &bloxide_core::spec::StateFns {
                on_entry: &[done_entry],
                on_exit: &[],
                transitions: &[TransitionRule {
                    event_tag: 2,
                    matches: |event: &TestEvent| matches!(event, TestEvent::GoRunning),
                    actions: &[] as &[ActionFn<Self>],
                    guard: |_ctx: &SpyCtx, _results: &ActionResults, _event: &TestEvent| {
                        Decision::Transition(LeafState::new(TestState::Running))
                    },
                }],
            },
        ];

        fn initial_state() -> TestState {
            TestState::Running
        }
        fn is_error(_state: &TestState) -> bool {
            false
        }
        fn on_init_entry(ctx: &mut SpyCtx) {
            init_entry(ctx);
        }
    }

    #[test]
    fn start_from_init_fires_on_entry() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        assert!(machine.current_state().is_init());
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
        assert!(matches!(
            outcome,
            DispatchOutcome::Started(MachineState::State(TestState::Running))
        ));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn start_from_operational_acknowledges_started() {
        // Redundant Start is acknowledged with Started (no callbacks, no
        // state change) — mirrors Stop-in-Init reporting Stopped.
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
        assert!(matches!(
            outcome,
            DispatchOutcome::Started(MachineState::State(TestState::Running))
        ));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn reset_goes_to_initial_state_with_exit_and_entry_chains() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));
        let outcome = machine.handle_lifecycle(LifecycleCommand::Reset);
        assert!(matches!(
            outcome,
            DispatchOutcome::Started(MachineState::State(TestState::Running))
        ));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 2);
        assert_eq!(machine.ctx().init_entry_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn stop_fires_exit_chain_and_reports_stopped() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        let outcome = machine.handle_lifecycle(LifecycleCommand::Stop);
        assert!(matches!(outcome, DispatchOutcome::Stopped));
        assert!(machine.current_state().is_init());
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().init_entry_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn ping_returns_alive_without_state_change() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        let outcome = machine.handle_lifecycle(LifecycleCommand::Ping);
        assert!(matches!(outcome, DispatchOutcome::Alive));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn transition_to_done_state_fires_on_entry() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        let outcome = machine.dispatch(TestEvent::Complete);
        assert!(matches!(
            outcome,
            DispatchOutcome::Transition(MachineState::State(TestState::Done))
        ));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Done)
        ));
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().done_entry_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn dispatch_with_lifecycle_event_variant_works() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        let outcome = machine.dispatch(TestEvent::Lifecycle(LifecycleCommand::Start));
        assert!(matches!(
            outcome,
            DispatchOutcome::Started(MachineState::State(TestState::Running))
        ));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));
    }

    #[test]
    fn done_state_can_transition_out_on_domain_event() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        machine.dispatch(TestEvent::Complete);
        assert_eq!(machine.ctx().done_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);
        let outcome = machine.dispatch(TestEvent::GoRunning);
        assert!(matches!(
            outcome,
            DispatchOutcome::Transition(MachineState::State(TestState::Running))
        ));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(TestState::Running)
        ));
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 2);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);
    }
}

// ── Fidelity tests (issue #135) ────────────────────────────────────────────

mod fidelity_tests {
    use crate::TestRuntime;
    use bloxide_core::capability::DynamicChannelCap;
    use bloxide_core::messaging::Envelope;
    use futures_core::Stream;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn tracking_waker() -> (Waker, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(false));
        fn make(flag: *const ()) -> RawWaker {
            unsafe fn clone(flag: *const ()) -> RawWaker {
                make(flag)
            }
            unsafe fn wake(flag: *const ()) {
                (*(flag as *const AtomicBool)).store(true, Ordering::SeqCst);
            }
            unsafe fn wake_by_ref(flag: *const ()) {
                (*(flag as *const AtomicBool)).store(true, Ordering::SeqCst);
            }
            unsafe fn drop_waker(_: *const ()) {}
            static VTABLE: RawWakerVTable =
                RawWakerVTable::new(clone, wake, wake_by_ref, drop_waker);
            RawWaker::new(flag, &VTABLE)
        }
        let raw = make(Arc::as_ptr(&flag) as *const ());
        (unsafe { Waker::from_raw(raw) }, flag)
    }

    #[test]
    fn capacity_enforced_on_try_send() {
        let (sender, mut rx) = TestRuntime::channel::<u32>(1, 2);
        sender.try_send(0, 1u32).unwrap();
        sender.try_send(0, 2u32).unwrap();
        assert!(
            sender.try_send(0, 3u32).is_err(),
            "third send beyond capacity 2 must fail"
        );
        let drained = rx.drain_payloads();
        assert_eq!(drained.len(), 2);
        sender
            .try_send(0, 4u32)
            .unwrap_or_else(|_| panic!("send after drain must succeed"));
    }

    #[test]
    fn zero_capacity_is_always_full() {
        let (sender, _rx) = TestRuntime::channel::<u32>(1, 0);
        assert!(
            sender.try_send(0, 1u32).is_err(),
            "capacity 0 must reject every try_send"
        );
    }

    #[test]
    fn close_after_last_sender_dropped_drains_then_none() {
        let (sender, mut rx) = TestRuntime::channel::<u32>(1, 4);
        sender.try_send(0, 7u32).unwrap();
        drop(sender);

        let (waker, _) = tracking_waker();
        let mut cx = Context::from_waker(&waker);
        // Queued envelope is delivered first…
        match Pin::new(&mut rx).poll_next(&mut cx) {
            Poll::Ready(Some(Envelope(_, 7))) => {}
            other => panic!("expected queued envelope, got {:?}", other),
        }
        // …then the closed channel reports Ready(None).
        match Pin::new(&mut rx).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            other => panic!("expected Ready(None) after close, got {:?}", other),
        }
        // Fused: stays Ready(None) on re-poll.
        match Pin::new(&mut rx).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            other => panic!("fused close must repeat Ready(None), got {:?}", other),
        }
    }

    #[test]
    fn sender_clones_keep_channel_open() {
        let (sender, mut rx) = TestRuntime::channel::<u32>(1, 4);
        let clone = sender.clone();
        drop(sender);

        let (waker, _) = tracking_waker();
        let mut cx = Context::from_waker(&waker);
        assert!(
            matches!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Pending),
            "one live clone must keep the channel open"
        );

        drop(clone);
        assert!(
            matches!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Ready(None)),
            "dropping the last clone must close the channel"
        );
    }

    #[test]
    fn dropping_last_sender_wakes_pending_receiver() {
        let (sender, mut rx) = TestRuntime::channel::<u32>(1, 4);
        let (waker, woken) = tracking_waker();
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(
            Pin::new(&mut rx).poll_next(&mut cx),
            Poll::Pending
        ));
        assert!(!woken.load(Ordering::SeqCst));

        drop(sender);
        assert!(
            woken.load(Ordering::SeqCst),
            "last-sender drop must wake the pending receiver"
        );
        assert!(matches!(
            Pin::new(&mut rx).poll_next(&mut cx),
            Poll::Ready(None)
        ));
    }

    #[test]
    fn receiver_drop_closes_send_side() {
        use bloxide_core::capability::BloxRuntime;

        let (sender, rx) = TestRuntime::channel::<u32>(1, 4);
        sender.try_send(0, 1u32).unwrap();
        drop(rx);

        let err = sender
            .try_send(0, 2u32)
            .expect_err("send after receiver drop must fail");
        assert_eq!(err, crate::TestTrySendError::Closed);
        assert!(
            TestRuntime::try_send_error_is_closed(&err),
            "Closed must classify as closed"
        );
        // Closed takes precedence even when capacity is available, and stays
        // closed (no resurrection).
        let err = sender.try_send(0, 3u32).expect_err("channel stays closed");
        assert_eq!(err, crate::TestTrySendError::Closed);
    }

    #[test]
    fn full_error_is_not_closed() {
        use bloxide_core::capability::BloxRuntime;

        let (sender, _rx) = TestRuntime::channel::<u32>(1, 0);
        let err = sender.try_send(0, 1u32).expect_err("capacity 0 is full");
        assert_eq!(err, crate::TestTrySendError::Full);
        assert!(
            !TestRuntime::try_send_error_is_closed(&err),
            "Full must not classify as closed"
        );
    }
}

// ── Observable kill tests ─────────────────────────────────────────────────

mod kill_tests {
    use crate::TestRuntime;
    use bloxide_spawn::SpawnCap;

    #[test]
    fn spawn_returns_increasing_ids_and_kill_is_recorded() {
        let before = crate::kill_count();
        let h1 = TestRuntime::spawn(async {});
        let h2 = TestRuntime::spawn(async {});
        assert!(h2 > h1, "spawn ids must increase: {} then {}", h1, h2);

        let kh1 = TestRuntime::kill_handle(h1);
        let kh2 = TestRuntime::kill_handle(h2);
        TestRuntime::kill(kh1);
        TestRuntime::kill(kh2);

        let killed = crate::drain_killed();
        assert_eq!(&killed[killed.len() - 2..], &[kh1, kh2]);
        let _ = before; // count is drain-relative; ids asserted above
    }
}
