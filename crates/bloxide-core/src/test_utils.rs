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
use std::task::{Context, Poll};
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
        }
    }
}

unsafe impl<M: Send + 'static> Send for TestSender<M> {}
unsafe impl<M: Send + 'static> Sync for TestSender<M> {}

pub struct TestReceiver<M: Send + 'static> {
    queue: Queue<M>,
}

impl<M: Send + 'static> TestReceiver<M> {
    pub fn drain_payloads(&mut self) -> Vec<M> {
        let mut lock = self.queue.lock().unwrap();
        lock.drain(..).map(|e| e.1).collect()
    }

    pub fn drain_envelopes(&mut self) -> Vec<Envelope<M>> {
        let mut lock = self.queue.lock().unwrap();
        lock.drain(..).collect()
    }
}

impl<M: Send + 'static> Stream for TestReceiver<M> {
    type Item = Envelope<M>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut lock = self.queue.lock().unwrap();
        match lock.pop_front() {
            Some(env) => Poll::Ready(Some(env)),
            None => Poll::Pending,
        }
    }
}

impl<M: Send + 'static> Unpin for TestReceiver<M> {}

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
        sender.queue.lock().unwrap().push_back(envelope);
        Ok(())
    }

    fn try_send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::TrySendError> {
        if sender.full.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(TestTrySendError);
        }
        sender.queue.lock().unwrap().push_back(envelope);
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
        let sender = TestSender {
            queue: Arc::clone(&queue),
            full: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let receiver = TestReceiver { queue };
        (ActorRef::new(id, sender), receiver)
    }
}
