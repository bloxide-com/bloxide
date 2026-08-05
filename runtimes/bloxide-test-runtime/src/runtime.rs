// Copyright 2025 Bloxide, all rights reserved

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
        // Take the queue lock so the close is atomic with respect to
        // try_send's check-and-enqueue critical section: a message can never
        // be queued after the receiver is gone.
        let _lock = self.shared.queue.lock();
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
        // The liveness check must be inside the queue lock: `TestReceiver::drop`
        // takes the same lock before clearing `receiver_alive`, so the check and
        // the enqueue are atomic with respect to the receiver going away.
        let mut lock = sender.shared.queue.lock();
        if !sender.shared.receiver_alive.load(Ordering::SeqCst) {
            return Err(TestTrySendError::Closed);
        }
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
