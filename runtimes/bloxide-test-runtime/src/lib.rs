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
//!
//! # Fidelity model (issue #135)
//!
//! Channels model the semantics the runtimes provide in production:
//!
//! - **Capacity** — `channel(id, capacity)` bounds `try_send`: it fails with
//!   `TestTrySendError::Full` once `capacity` envelopes are queued.
//!   `capacity = 0` means every `try_send` fails (always-full channel).
//! - **Close semantics** — the channel tracks its sender count. When the last
//!   `TestSender` (including every `ActorRef` clone) is dropped, the receiver
//!   drains any queued envelopes and then returns `Poll::Ready(None)` — the
//!   all-streams-close behavior of issue #134. Dropping the last sender also
//!   wakes a pending receiver so it observes the close.
//! - **Receiver liveness** — dropping the `TestReceiver` closes the send side:
//!   `try_send` fails with `TestTrySendError::Closed` (Tokio semantics), which
//!   `try_send_error_is_closed` classifies as closed. This lets tests drive
//!   the confirm-before-record supervision paths that key off dead channels.
//! - **Observable kill** — `SpawnCap` uses `usize` handles (a monotonically
//!   increasing spawn id). `kill` records the id in a thread-local log;
//!   `drain_killed()` / `kill_count()` let tests assert the kill path fired.
//!
//! # Intentional gaps
//!
//! - `send_via` is **unbounded** (no backpressure) — action functions all use
//!   `try_send`, so `try_send` is the backpressure path under test.
//! - Spawned futures are **recorded, not executed** — `kill` only logs the id;
//!   it does not (and cannot) drop the recorded future.
//!
//! # `no_std` support
//!
//! The crate is `no_std` + `alloc`. With the default `std` feature the spawn
//! and kill logs are thread-local (parallel `cargo test` threads stay
//! isolated); with `--no-default-features --features alloc` they become
//! process-global logs behind a `spin::Mutex` (single-threaded harnesses).

#![no_std]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

use bloxide_core::capability::{
    BloxRuntime, DynamicChannelCap, GroupChannelCap, DYNAMIC_ACTOR_ID_BASE,
};
use bloxide_core::messaging::{ActorId, ActorRef, Envelope};
use bloxide_spawn::{Kill, SpawnCap};

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::task::{Context, Poll, Waker};
use futures_core::Stream;
use spin::Mutex;

// ── Unique actor ID generator ────────────────────────────────────────────

// Starts at `DYNAMIC_ACTOR_ID_BASE` so runtime-allocated IDs can never
// collide with the compile-time counter used by `channels!` / `next_actor_id!`.
static NEXT_ID: AtomicUsize = AtomicUsize::new(DYNAMIC_ACTOR_ID_BASE);

fn alloc_test_id() -> ActorId {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

// ── Shared channel state ─────────────────────────────────────────────────

struct Shared<M: Send + 'static> {
    queue: Mutex<VecDeque<Envelope<M>>>,
    waker: Mutex<Option<Waker>>,
    /// Live sender count (initial sender + every clone). Close happens at 0.
    sender_count: AtomicUsize,
    /// Receiver liveness. `false` once the `TestReceiver` is dropped —
    /// `try_send` then fails with `TestTrySendError::Closed`.
    receiver_alive: AtomicBool,
    /// Maximum queued envelopes before `try_send` fails.
    capacity: usize,
}

impl<M: Send + 'static> Shared<M> {
    fn wake(&self) {
        if let Some(waker) = self.waker.lock().take() {
            waker.wake();
        }
    }
}

pub struct TestSender<M: Send + 'static> {
    shared: Arc<Shared<M>>,
}

impl<M: Send + 'static> Clone for TestSender<M> {
    fn clone(&self) -> Self {
        self.shared.sender_count.fetch_add(1, Ordering::SeqCst);
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<M: Send + 'static> Drop for TestSender<M> {
    fn drop(&mut self) {
        if self.shared.sender_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            // Last sender dropped — wake the receiver so it observes the close.
            self.shared.wake();
        }
    }
}

pub struct TestReceiver<M: Send + 'static> {
    shared: Arc<Shared<M>>,
}

impl<M: Send + 'static> TestReceiver<M> {
    pub fn drain_payloads(&mut self) -> Vec<M> {
        let mut lock = self.shared.queue.lock();
        lock.drain(..).map(|e| e.1).collect()
    }

    pub fn drain_envelopes(&mut self) -> Vec<Envelope<M>> {
        let mut lock = self.shared.queue.lock();
        lock.drain(..).collect()
    }
}

impl<M: Send + 'static> Drop for TestReceiver<M> {
    fn drop(&mut self) {
        // Receiver gone — the send side observes Closed on the next try_send.
        self.shared.receiver_alive.store(false, Ordering::SeqCst);
    }
}

impl<M: Send + 'static> Stream for TestReceiver<M> {
    type Item = Envelope<M>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        {
            let mut lock = self.shared.queue.lock();
            if let Some(env) = lock.pop_front() {
                return Poll::Ready(Some(env));
            }
            if self.shared.sender_count.load(Ordering::SeqCst) == 0 {
                // All senders dropped and queue drained — channel closed (fused:
                // this keeps returning Ready(None) on re-poll).
                return Poll::Ready(None);
            }
        }
        // Register the waker, then re-check the queue: a send that landed
        // between the first check and registration must not be lost (the
        // sender's wake() would have found an empty waker slot).
        *self.shared.waker.lock() = Some(cx.waker().clone());
        let mut lock = self.shared.queue.lock();
        if let Some(env) = lock.pop_front() {
            return Poll::Ready(Some(env));
        }
        if self.shared.sender_count.load(Ordering::SeqCst) == 0 {
            return Poll::Ready(None);
        }
        Poll::Pending
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

/// Error returned by `TestRuntime::try_send_via`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestTrySendError {
    /// Capacity exhausted — transient; the receiver may drain later.
    Full,
    /// Receiver dropped — the channel is permanently dead.
    Closed,
}

impl core::fmt::Display for TestTrySendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Full => write!(f, "test try_send error: channel full"),
            Self::Closed => write!(f, "test try_send error: channel closed"),
        }
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
    type Kill = Kill;

    fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M> {
        rx
    }

    async fn send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::SendError> {
        // Intentional gap: unbounded (no backpressure on the async path).
        sender.shared.queue.lock().push_back(envelope);
        sender.shared.wake();
        Ok(())
    }

    fn try_send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::TrySendError> {
        if !sender.shared.receiver_alive.load(Ordering::SeqCst) {
            return Err(TestTrySendError::Closed);
        }
        let mut lock = sender.shared.queue.lock();
        if lock.len() >= sender.shared.capacity {
            return Err(TestTrySendError::Full);
        }
        lock.push_back(envelope);
        drop(lock);
        sender.shared.wake();
        Ok(())
    }

    fn try_send_error_is_closed(err: &Self::TrySendError) -> bool {
        matches!(err, TestTrySendError::Closed)
    }
}

impl DynamicChannelCap for TestRuntime {
    fn alloc_actor_id() -> ActorId {
        alloc_test_id()
    }

    fn channel<M: Send + 'static>(
        id: ActorId,
        capacity: usize,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            waker: Mutex::new(None),
            sender_count: AtomicUsize::new(1),
            receiver_alive: AtomicBool::new(true),
            capacity,
        });
        let sender = TestSender {
            shared: Arc::clone(&shared),
        };
        let receiver = TestReceiver { shared };
        (ActorRef::new(id, sender), receiver)
    }
}

// ── GroupChannelCap ──────────────────────────────────────────────────────

impl GroupChannelCap for TestRuntime {
    fn alloc_group_id() -> ActorId {
        alloc_test_id()
    }

    fn group_channel<M: Send + 'static, const N: usize>(
        id: ActorId,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
        <Self as DynamicChannelCap>::channel::<M>(id, N)
    }
}

// ── SpawnCap ─────────────────────────────────────────────────────────────

use alloc::boxed::Box;
use core::future::Future;

type SpawnedVec = Vec<Pin<Box<dyn Future<Output = ()> + Send>>>;

#[cfg(feature = "std")]
mod spawn_log {
    //! Thread-local spawn/kill logs: parallel `cargo test` threads each get
    //! their own log, so tests cannot interfere with one another.
    use super::SpawnedVec;
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    use core::cell::{Cell, RefCell};
    use core::future::Future;
    use core::pin::Pin;

    std::thread_local! {
        static SPAWNED: RefCell<SpawnedVec> = RefCell::new(Vec::new());
        /// Monotonically increasing spawn id — stable across `drain_spawned` calls,
        /// so a `KillHandle` stays correlated with its spawn.
        static NEXT_SPAWN_ID: Cell<usize> = const { Cell::new(0) };
        /// Ids of tasks killed via `SpawnCap::kill` since the last `drain_killed`.
        static KILLED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    }

    pub fn next_spawn_id() -> usize {
        NEXT_SPAWN_ID.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        })
    }

    pub fn record_spawn(future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        SPAWNED.with(|s| s.borrow_mut().push(future));
    }

    pub fn record_kill(handle: usize) {
        KILLED.with(|k| k.borrow_mut().push(handle));
    }

    pub fn drain_spawned() -> SpawnedVec {
        SPAWNED.with(|s| s.borrow_mut().drain(..).collect())
    }

    pub fn spawned_count() -> usize {
        SPAWNED.with(|s| s.borrow().len())
    }

    pub fn drain_killed() -> Vec<usize> {
        KILLED.with(|k| k.borrow_mut().drain(..).collect())
    }

    pub fn kill_count() -> usize {
        KILLED.with(|k| k.borrow().len())
    }
}

#[cfg(not(feature = "std"))]
mod spawn_log {
    //! `no_std` fallback: process-global logs behind a `spin::Mutex`. There
    //! are no threads to isolate in an alloc-only harness, so a single
    //! global log is sufficient.
    use super::SpawnedVec;
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    use core::future::Future;
    use core::pin::Pin;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use spin::Mutex;

    static SPAWNED: Mutex<SpawnedVec> = Mutex::new(Vec::new());
    static NEXT_SPAWN_ID: AtomicUsize = AtomicUsize::new(0);
    static KILLED: Mutex<Vec<usize>> = Mutex::new(Vec::new());

    pub fn next_spawn_id() -> usize {
        NEXT_SPAWN_ID.fetch_add(1, Ordering::Relaxed)
    }

    pub fn record_spawn(future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        SPAWNED.lock().push(future);
    }

    pub fn record_kill(handle: usize) {
        KILLED.lock().push(handle);
    }

    pub fn drain_spawned() -> SpawnedVec {
        SPAWNED.lock().drain(..).collect()
    }

    pub fn spawned_count() -> usize {
        SPAWNED.lock().len()
    }

    pub fn drain_killed() -> Vec<usize> {
        KILLED.lock().drain(..).collect()
    }

    pub fn kill_count() -> usize {
        KILLED.lock().len()
    }
}

impl SpawnCap for TestRuntime {
    type TaskHandle = usize;
    type KillHandle = usize;

    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self::TaskHandle {
        let id = spawn_log::next_spawn_id();
        spawn_log::record_spawn(Box::pin(future));
        id
    }

    fn kill_handle(handle: Self::TaskHandle) -> Self::KillHandle {
        handle
    }

    fn kill(handle: Self::KillHandle) {
        // Recorded, not executed: TestRuntime doesn't run real tasks, but the
        // kill path is observable — tests assert via `drain_killed`/`kill_count`.
        spawn_log::record_kill(handle);
    }
}

/// Drain all futures submitted via `SpawnCap::spawn` since the last drain.
pub fn drain_spawned() -> SpawnedVec {
    spawn_log::drain_spawned()
}

/// Returns the number of futures submitted since the last drain.
pub fn spawned_count() -> usize {
    spawn_log::spawned_count()
}

/// Drain all spawn ids recorded by `SpawnCap::kill` since the last drain.
pub fn drain_killed() -> Vec<usize> {
    spawn_log::drain_killed()
}

/// Returns the number of kills recorded since the last `drain_killed`.
pub fn kill_count() -> usize {
    spawn_log::kill_count()
}

// ── Spawn helper tests ─────────────────────────────────────────────────────
//
// `spawn_dynamic_child` (bloxide-spawn) tests live here rather than in bloxide-spawn:
// a bloxide-spawn dev-dependency on this crate would be a dev-dependency
// cycle (two non-unifying `bloxide-spawn` instances in the graph).

#[cfg(test)]
mod spawn_helper_tests;

// ── Waker tests ──────────────────────────────────────────────────────────

#[cfg(test)]
// TODO: wire these up — several `WEvent` variants/fields below exist only to
// satisfy the `EventTag`/`LifecycleEvent`/`From<Envelope>` trait impls and are
// not directly constructed or read in the waker tests yet.
#[allow(dead_code)]
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
                            Decision::Stop
                        } else {
                            Decision::Stay
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
            sender_clone.try_send(0, 42u32).unwrap();
        });

        block_on(run(machine, (stream,), RunConfig::<TestRuntime>::bare(), 0));

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

#[cfg(test)]
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
        // TODO: wire this up — `Msg` is constructed by `From<Envelope<u32>>`
        // (required for the mailbox type) but no test in this module dispatches
        // a domain `Msg` event, so the payload field is never read.
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

#[cfg(test)]
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

#[cfg(test)]
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
