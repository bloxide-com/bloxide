// Copyright 2025 Bloxide, all rights reserved
//! Test runtime: in-memory channels and a manual virtual clock.
//!
//! `TestRuntime` implements `BloxRuntime` (from `bloxide-core`) and `SpawnCap`
//! (from `bloxide-spawn`) so it can be used as the `R` type parameter in unit
//! tests without an Embassy or Tokio executor.
//!
//! Timer simulation is intentionally not part of `TestRuntime` itself.
//! Tests that use timers should pair `TestRuntime` with `bloxide_timer::test_utils`
//! instead.

extern crate alloc;

use bloxide_core::capability::{BloxRuntime, DynamicChannelCap};
use bloxide_core::messaging::{ActorId, ActorRef, Envelope};
use bloxide_spawn::{Kill, SpawnCap};

use futures_core::Stream;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::vec::Vec;

// ── Unique actor ID generator ────────────────────────────────────────────

static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

fn alloc_test_id() -> ActorId {
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

// ── Shared in-memory queue ───────────────────────────────────────────────

type Queue<M> = Arc<Mutex<VecDeque<Envelope<M>>>>;

pub struct TestSender<M: Send + 'static> {
    queue: Queue<M>,
    full: Arc<std::sync::atomic::AtomicBool>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl<M: Send + 'static> TestSender<M> {
    /// When set to `true`, subsequent `try_send` calls will return an error.
    pub fn set_full(&self, full: bool) {
        self.full.store(full, std::sync::atomic::Ordering::Relaxed);
    }
}

impl<M: Send + 'static> Clone for TestSender<M> {
    fn clone(&self) -> Self {
        Self {
            queue: Arc::clone(&self.queue),
            full: Arc::clone(&self.full),
            waker: Arc::clone(&self.waker),
        }
    }
}

pub struct TestReceiver<M: Send + 'static> {
    queue: Queue<M>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl<M: Send + 'static> TestReceiver<M> {
    pub fn drain_payloads(&mut self) -> Vec<M> {
        let mut lock = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        lock.drain(..).map(|e| e.1).collect()
    }

    pub fn drain_envelopes(&mut self) -> Vec<Envelope<M>> {
        let mut lock = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        lock.drain(..).collect()
    }
}

impl<M: Send + 'static> Stream for TestReceiver<M> {
    type Item = Envelope<M>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut lock = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        match lock.pop_front() {
            Some(env) => Poll::Ready(Some(env)),
            None => {
                *self.waker.lock().unwrap_or_else(|e| e.into_inner()) = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// Error returned by `TestRuntime::send_via` (always succeeds in test).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestSendError;

impl core::fmt::Display for TestSendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "test send error (should never occur)")
    }
}

impl std::error::Error for TestSendError {}

/// Error returned by `TestRuntime::try_send_via` when capacity is exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestTrySendError;

impl core::fmt::Display for TestTrySendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "test try_send error: channel full")
    }
}

impl std::error::Error for TestTrySendError {}

// ── TestRuntime ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TestRuntime;

impl TestRuntime {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TestRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl BloxRuntime for TestRuntime {
    type SendError = TestSendError;
    type TrySendError = TestTrySendError;
    type Sender<M: Send + 'static> = TestSender<M>;
    type Receiver<M: Send + 'static> = TestReceiver<M>;
    type Stream<M: Send + 'static> = TestReceiver<M>;
    type Kill = Kill;

    fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M> {
        rx
    }

    async fn send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::SendError> {
        sender
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(envelope);
        if let Some(waker) = sender
            .waker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            waker.wake();
        }
        Ok(())
    }

    fn try_send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::TrySendError> {
        if sender.full.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(TestTrySendError);
        }
        sender
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(envelope);
        if let Some(waker) = sender
            .waker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            waker.wake();
        }
        Ok(())
    }
}

impl DynamicChannelCap for TestRuntime {
    fn alloc_actor_id() -> ActorId {
        alloc_test_id()
    }

    fn channel<M: Send + 'static>(
        id: ActorId,
        _capacity: usize,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
        let queue: Queue<M> = Arc::new(Mutex::new(VecDeque::new()));
        let waker: Arc<Mutex<Option<Waker>>> = Arc::new(Mutex::new(None));
        let sender = TestSender {
            queue: Arc::clone(&queue),
            full: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            waker: Arc::clone(&waker),
        };
        let receiver = TestReceiver { queue, waker };
        (ActorRef::new(id, sender), receiver)
    }
}

// ── SpawnCap ─────────────────────────────────────────────────────────────

use alloc::boxed::Box;
use alloc::vec::Vec as AllocVec;
use core::future::Future;

type SpawnedVec = AllocVec<Pin<Box<dyn Future<Output = ()> + Send>>>;

thread_local! {
    static SPAWNED: std::cell::RefCell<SpawnedVec> = std::cell::RefCell::new(AllocVec::new());
}

impl SpawnCap for TestRuntime {
    type TaskHandle = ();
    type KillHandle = ();

    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self::TaskHandle {
        SPAWNED.with(|s| s.borrow_mut().push(Box::pin(future)));
    }

    fn kill_handle(_handle: Self::TaskHandle) -> Self::KillHandle {
        // No-op: TestRuntime doesn't run real tasks
    }

    fn kill(_handle: Self::KillHandle) {
        // No-op: TestRuntime doesn't run real tasks
    }
}

/// Drain all futures submitted via `SpawnCap::spawn` since the last drain.
pub fn drain_spawned() -> SpawnedVec {
    SPAWNED.with(|s| s.borrow_mut().drain(..).collect())
}

/// Returns the number of futures submitted since the last drain.
pub fn spawned_count() -> usize {
    SPAWNED.with(|s| s.borrow().len())
}

// ── Waker tests ──────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(dead_code)]
mod waker_tests {
    use bloxide_core::actor::run_actor_to_completion;
    use bloxide_core::capability::{BloxRuntime, DynamicChannelCap};
    use bloxide_core::engine::StateMachine;
    use bloxide_core::event_tag::{EventTag, LifecycleEvent};
    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::messaging::Envelope;
    use bloxide_core::spec::MachineSpec;
    use bloxide_core::topology::{LeafState, StateTopology};
    use bloxide_core::transition::{ActionResult, Guard, TransitionRule};
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
        Done,
    }

    impl StateTopology for WState {
        const STATE_COUNT: usize = 3;
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
                WState::Done => &[WState::Done],
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
                    matches: |ev| matches!(ev, WEvent::Msg(_)),
                    actions: &[|ctx, _ev| {
                        ctx.processed.fetch_add(1, Ordering::SeqCst);
                        ActionResult::Ok
                    }],
                    guard: |ctx, _results, _ev| {
                        if ctx.processed.load(Ordering::SeqCst) >= ctx.threshold {
                            Guard::Transition(LeafState::new(WState::Done))
                        } else {
                            Guard::Stay
                        }
                    },
                }],
            },
            &bloxide_core::spec::StateFns {
                on_entry: &[],
                on_exit: &[],
                transitions: &[],
            },
        ];

        fn initial_state() -> WState {
            WState::Running
        }

        fn is_terminal(state: &WState) -> bool {
            matches!(state, WState::Done)
        }
    }

    #[test]
    fn test_runtime_async_wakeup() {
        let id = TestRuntime::alloc_actor_id();
        let (sender_ref, receiver) = TestRuntime::channel::<u32>(id, 16);
        let stream = TestRuntime::to_stream(receiver);

        let processed = Arc::new(AtomicU32::new(0));
        let ctx = WCtx {
            processed: processed.clone(),
            threshold: 1,
        };
        let mut machine = StateMachine::<WSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);

        let sender_clone = sender_ref.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            sender_clone.try_send(0, 42u32).unwrap();
        });

        block_on(run_actor_to_completion(machine, (stream,)));

        handle.join().unwrap();
        assert_eq!(processed.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_runtime_waker_idempotent() {
        let id = TestRuntime::alloc_actor_id();
        let (sender_ref, receiver) = TestRuntime::channel::<u32>(id, 16);
        let stream = TestRuntime::to_stream(receiver);

        let processed = Arc::new(AtomicU32::new(0));
        let ctx = WCtx {
            processed: processed.clone(),
            threshold: 5,
        };
        let mut machine = StateMachine::<WSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);

        let sender_clone = sender_ref.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            for i in 0..5u32 {
                sender_clone.try_send(0, i).unwrap();
            }
        });

        block_on(run_actor_to_completion(machine, (stream,)));

        handle.join().unwrap();
        assert_eq!(processed.load(Ordering::SeqCst), 5);
    }
}

// ── Lifecycle dispatch tests ──────────────────────────────────────────────
//
// Moved from bloxide-core to avoid circular dev-dependency.
// These tests verify lifecycle command dispatch through the engine using
// TestRuntime as the concrete runtime.

#[cfg(test)]
mod lifecycle_dispatch {
    use bloxide_core::engine::{DispatchOutcome, MachineState, StateMachine};
    use bloxide_core::event_tag::{EventTag, LifecycleEvent};
    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::messaging::Envelope;
    use bloxide_core::spec::MachineSpec;
    use bloxide_core::topology::{LeafState, StateTopology};
    use bloxide_core::transition::{ActionFn, ActionResults, Guard, TransitionRule};
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
        fn parent(self) -> Option<Self> { None }
        fn is_leaf(self) -> bool { true }
        fn path(self) -> &'static [Self] {
            match self {
                TestState::Init => &[TestState::Init],
                TestState::Running => &[TestState::Running],
                TestState::Done => &[TestState::Done],
            }
        }
        fn as_index(self) -> usize { self as usize }
    }

    #[derive(Debug, Clone, Copy)]
    enum TestEvent {
        Lifecycle(LifecycleCommand),
        #[allow(dead_code)]
        Msg(u32),
        Complete,
        GoRunning,
    }

    impl EventTag for TestEvent {
        fn event_tag(&self) -> u8 {
            match self {
                TestEvent::Lifecycle(_) => 254,
                TestEvent::Msg(_) => 0,
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
        fn from(env: Envelope<u32>) -> Self { TestEvent::Msg(env.1) }
    }

    #[derive(Default)]
    struct SpyCtx {
        running_entry_count: Arc<AtomicU32>,
        running_exit_count: Arc<AtomicU32>,
        done_entry_count: Arc<AtomicU32>,
        init_entry_count: Arc<AtomicU32>,
    }

    struct TestSpec<R>(PhantomData<R>);

    fn running_entry(ctx: &mut SpyCtx) { ctx.running_entry_count.fetch_add(1, Ordering::SeqCst); }
    fn running_exit(ctx: &mut SpyCtx) { ctx.running_exit_count.fetch_add(1, Ordering::SeqCst); }
    fn done_entry(ctx: &mut SpyCtx) { ctx.done_entry_count.fetch_add(1, Ordering::SeqCst); }
    fn init_entry(ctx: &mut SpyCtx) { ctx.init_entry_count.fetch_add(1, Ordering::SeqCst); }

    impl<R: bloxide_core::capability::BloxRuntime> MachineSpec for TestSpec<R> {
        type State = TestState;
        type Event = TestEvent;
        type Ctx = SpyCtx;
        type Mailboxes<Rt: bloxide_core::capability::BloxRuntime> = (Rt::Stream<u32>,);

        const HANDLER_TABLE: &'static [&'static bloxide_core::spec::StateFns<Self>] = &[
            &bloxide_core::spec::StateFns { on_entry: &[], on_exit: &[], transitions: &[] },
            &bloxide_core::spec::StateFns {
                on_entry: &[running_entry],
                on_exit: &[running_exit],
                transitions: &[TransitionRule {
                    event_tag: 1,
                    matches: |event: &TestEvent| matches!(event, TestEvent::Complete),
                    actions: &[] as &[ActionFn<Self>],
                    guard: |_ctx: &SpyCtx, _results: &ActionResults, _event: &TestEvent| {
                        Guard::Transition(LeafState::new(TestState::Done))
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
                        Guard::Transition(LeafState::new(TestState::Running))
                    },
                }],
            },
        ];

        fn initial_state() -> TestState { TestState::Running }
        fn is_terminal(state: &TestState) -> bool { matches!(state, TestState::Done) }
        fn is_error(_state: &TestState) -> bool { false }
        fn on_init_entry(ctx: &mut SpyCtx) { init_entry(ctx); }
    }

    #[test]
    fn start_from_init_fires_on_entry() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        assert!(machine.current_state().is_init());
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
        assert!(matches!(outcome, DispatchOutcome::Started(MachineState::State(TestState::Running))));
        assert!(matches!(machine.current_state(), MachineState::State(TestState::Running)));
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn start_from_operational_is_noop() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
        assert!(matches!(outcome, DispatchOutcome::HandledNoTransition));
        assert!(matches!(machine.current_state(), MachineState::State(TestState::Running)));
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn reset_goes_to_initial_state_with_exit_and_entry_chains() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        assert!(matches!(machine.current_state(), MachineState::State(TestState::Running)));
        let outcome = machine.handle_lifecycle(LifecycleCommand::Reset);
        assert!(matches!(outcome, DispatchOutcome::Started(MachineState::State(TestState::Running))));
        assert!(matches!(machine.current_state(), MachineState::State(TestState::Running)));
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
        assert!(matches!(machine.current_state(), MachineState::State(TestState::Running)));
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn transition_to_terminal_state_fires_on_entry_and_reports_done() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        let outcome = machine.dispatch(TestEvent::Complete);
        assert!(matches!(outcome, DispatchOutcome::Done(MachineState::State(TestState::Done))));
        assert!(matches!(machine.current_state(), MachineState::State(TestState::Done)));
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().done_entry_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn dispatch_with_lifecycle_event_variant_works() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        let outcome = machine.dispatch(TestEvent::Lifecycle(LifecycleCommand::Start));
        assert!(matches!(outcome, DispatchOutcome::Started(MachineState::State(TestState::Running))));
        assert!(matches!(machine.current_state(), MachineState::State(TestState::Running)));
    }

    #[test]
    fn terminal_state_cannot_transition_out_on_domain_event() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        machine.dispatch(TestEvent::Complete);
        assert_eq!(machine.ctx().done_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);
        let outcome = machine.dispatch(TestEvent::GoRunning);
        assert!(matches!(outcome, DispatchOutcome::HandledNoTransition));
        assert!(matches!(machine.current_state(), MachineState::State(TestState::Done)));
        assert_eq!(machine.ctx().running_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().done_entry_count.load(Ordering::SeqCst), 1);
        assert_eq!(machine.ctx().running_exit_count.load(Ordering::SeqCst), 1);
    }
}
