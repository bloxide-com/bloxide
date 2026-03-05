// Copyright 2025 Bloxide, all rights reserved
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    messaging::{ActorId, ActorRef, Envelope},
};
use tokio::sync::mpsc;

use crate::{
    channel::{TokioSendError, TokioSender, TokioStream, TokioTrySendError},
    TokioRuntime,
};

// ── Actor ID allocation ───────────────────────────────────────────────────────

static NEXT_TOKIO_ID: AtomicUsize = AtomicUsize::new(1);

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
        sender
            .inner
            .try_send(envelope)
            .map_err(|_| TokioTrySendError)
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
