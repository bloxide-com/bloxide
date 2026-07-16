// Copyright 2025 Bloxide, all rights reserved
/// Test runtime: in-memory channels and a manual virtual clock.
///
/// Enabled by the `std` feature (tests always build with std on the host).
/// `TestRuntime` implements `BloxRuntime` so it can be used as
/// the `R` type parameter in unit tests without an Embassy or Tokio executor.
///
/// Timer simulation is intentionally not part of `TestRuntime` itself.
/// `bloxide-core` cannot depend on `bloxide-timer`, so shared timer harnesses
/// live in `bloxide_timer::test_utils` instead. Tests that use timers should
/// pair `TestRuntime` with that helper rather than re-implementing timer queues
/// inline.
use crate::capability::BloxRuntime;
use crate::messaging::{ActorId, ActorRef, Envelope};
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

#[cfg(feature = "std")]
impl std::error::Error for TestSendError {}

/// Error returned by `TestRuntime::try_send_via` when capacity is exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestTrySendError;

impl core::fmt::Display for TestTrySendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "test try_send error: channel full")
    }
}

#[cfg(feature = "std")]
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

impl crate::capability::DynamicChannelCap for TestRuntime {
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
        let receiver = TestReceiver {
            queue,
            waker,
        };
        (ActorRef::new(id, sender), receiver)
    }
}

// ── SpawnCap test helpers ──────────────────────────────────────────────────
#[cfg(feature = "std")]
mod spawn_helpers {
    use crate::capability::SpawnCap;
    use crate::test_utils::TestRuntime;
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    use core::future::Future;
    use core::pin::Pin;
    use std::cell::RefCell;
    use std::thread_local;

    type SpawnedVec = Vec<Pin<Box<dyn Future<Output = ()> + Send>>>;

    thread_local! {
        static SPAWNED: RefCell<SpawnedVec> = RefCell::new(Vec::new());
    }

    impl SpawnCap for TestRuntime {
        fn spawn(future: impl Future<Output = ()> + Send + 'static) {
            SPAWNED.with(|s| s.borrow_mut().push(Box::pin(future)));
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
}

#[cfg(feature = "std")]
pub use spawn_helpers::{drain_spawned, spawned_count};

#[cfg(all(test, feature = "std"))]
#[allow(dead_code)]
mod waker_tests {
    use crate::capability::{BloxRuntime, DynamicChannelCap};
    use crate::actor::run_actor_to_completion;
    use crate::engine::StateMachine;
    use crate::event_tag::{EventTag, LifecycleEvent};
    use crate::lifecycle::LifecycleCommand;
    use crate::messaging::Envelope;
    use crate::spec::MachineSpec;
    use crate::topology::{LeafState, StateTopology};
    use crate::transition::{ActionResult, Guard, TransitionRule};
    use std::marker::PhantomData;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    /// Minimal block_on that only re-polls when woken.
    ///
    /// This is critical for testing the waker mechanism: if poll_next does not
    /// store the waker, the wake() call from send_via/try_send_via will not
    /// set the flag, and block_on will hang forever.
    fn block_on<F: core::future::Future>(future: F) -> F::Output {
        struct WakeFlag {
            woken: AtomicBool,
        }

        fn make_raw_waker(flag: *const WakeFlag) -> RawWaker {
            unsafe fn clone(flag: *const ()) -> RawWaker {
                make_raw_waker(flag as *const WakeFlag)
            }
            unsafe fn wake(flag: *const ()) {
                (*(flag as *const WakeFlag)).woken.store(true, Ordering::SeqCst);
            }
            unsafe fn wake_by_ref(flag: *const ()) {
                (*(flag as *const WakeFlag)).woken.store(true, Ordering::SeqCst);
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

    // ── Test state machine ──────────────────────────────────────────────

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

    impl<R: crate::capability::BloxRuntime> MachineSpec for WSpec<R> {
        type State = WState;
        type Event = WEvent;
        type Ctx = WCtx;
        type Mailboxes<Rt: crate::capability::BloxRuntime> = (Rt::Stream<u32>,);

        const HANDLER_TABLE: &'static [&'static crate::spec::StateFns<Self>] = &[
            // Init
            &crate::spec::StateFns {
                on_entry: &[],
                on_exit: &[],
                transitions: &[],
            },
            // Running - transition to Done after threshold messages
            &crate::spec::StateFns {
                on_entry: &[],
                on_exit: &[],
                transitions: &[TransitionRule {
                    event_tag: 0, // WEvent::Msg tag
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
            // Done (terminal)
            &crate::spec::StateFns {
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
        let id = super::TestRuntime::alloc_actor_id();
        let (sender_ref, receiver) = super::TestRuntime::channel::<u32>(id, 16);
        let stream = super::TestRuntime::to_stream(receiver);

        let processed = Arc::new(AtomicU32::new(0));
        let ctx = WCtx {
            processed: processed.clone(),
            threshold: 1,
        };
        let mut machine = StateMachine::<WSpec<super::TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);

        // Send a message from another thread after a delay.
        // The actor will be waiting (queue empty, waker stored).
        // When try_send pushes and wakes, block_on re-polls and the actor processes it.
        let sender_clone = sender_ref.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            sender_clone.try_send(0, 42u32).unwrap();
        });

        // This will hang forever if the waker is not stored in poll_next.
        block_on(run_actor_to_completion(machine, (stream,)));

        handle.join().unwrap();
        assert_eq!(processed.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_runtime_waker_idempotent() {
        let id = super::TestRuntime::alloc_actor_id();
        let (sender_ref, receiver) = super::TestRuntime::channel::<u32>(id, 16);
        let stream = super::TestRuntime::to_stream(receiver);

        let processed = Arc::new(AtomicU32::new(0));
        let ctx = WCtx {
            processed: processed.clone(),
            threshold: 5,
        };
        let mut machine = StateMachine::<WSpec<super::TestRuntime>>::new(ctx);
        machine.handle_lifecycle(LifecycleCommand::Start);

        // Send multiple messages from another thread.
        // Each wake() should cause block_on to re-poll and process one message.
        // Multiple messages sent while the actor is waiting should all be processed.
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
