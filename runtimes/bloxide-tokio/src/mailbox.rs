// Copyright 2025 Bloxide, all rights reserved
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap, GroupChannelCap, DYNAMIC_ACTOR_ID_BASE},
    messaging::{ActorId, ActorRef, Envelope},
};
use bloxide_spawn::Kill;
use tokio::sync::mpsc;

use crate::{
    channel::{TokioSendError, TokioSender, TokioStream, TokioTrySendError},
    TokioRuntime,
};

// ── Actor ID allocation ───────────────────────────────────────────────────────

// Dynamic actor IDs (spawned at runtime) start well above the compile-time
// counter used by `channels!` / `next_actor_id!` so the two spaces can never
// collide (`DYNAMIC_ACTOR_ID_BASE`).
static NEXT_TOKIO_ID: AtomicUsize = AtomicUsize::new(DYNAMIC_ACTOR_ID_BASE);

fn alloc_tokio_id() -> ActorId {
    NEXT_TOKIO_ID.fetch_add(1, Ordering::Relaxed)
}

// ── BloxRuntime impl ──────────────────────────────────────────────────────────

impl BloxRuntime for TokioRuntime {
    type SendError = TokioSendError;
    type TrySendError = TokioTrySendError;
    type Sender<M: Send + 'static> = TokioSender<M>;
    type Receiver<M: Send + 'static> = TokioStream<M>;
    type Stream<M: Send + 'static> = TokioStream<M>;
    type Kill = Kill;

    fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M> {
        rx
    }

    async fn send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::SendError> {
        sender
            .inner
            .send(envelope)
            .await
            .map_err(|_| TokioSendError)
    }

    fn try_send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::TrySendError> {
        sender.inner.try_send(envelope).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => TokioTrySendError::Full,
            mpsc::error::TrySendError::Closed(_) => TokioTrySendError::Closed,
        })
    }

    fn try_send_error_is_closed(err: &Self::TrySendError) -> bool {
        matches!(err, TokioTrySendError::Closed)
    }

    async fn yield_now() {
        tokio::task::yield_now().await;
    }
}

// ── DynamicChannelCap impl ────────────────────────────────────────────────────

impl DynamicChannelCap for TokioRuntime {
    fn alloc_actor_id() -> ActorId {
        alloc_tokio_id()
    }

    fn channel<M: Send + 'static>(
        id: ActorId,
        capacity: usize,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
        let (tx, rx) = mpsc::channel::<Envelope<M>>(capacity);
        let sender = TokioSender {
            inner: Arc::new(tx),
        };
        let stream = TokioStream { inner: rx };
        (ActorRef::new(id, sender), stream)
    }
}

// ── GroupChannelCap impl ──────────────────────────────────────────────────────

impl GroupChannelCap for TokioRuntime {
    fn alloc_group_id() -> ActorId {
        alloc_tokio_id()
    }

    fn group_channel<M: Send + 'static, const N: usize>(
        id: ActorId,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
        <Self as DynamicChannelCap>::channel::<M>(id, N)
    }
}
