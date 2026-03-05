use bloxide_core::messaging::Envelope;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::Stream;
use std::sync::Arc;
use tokio::sync::mpsc;

// ── TokioSender ───────────────────────────────────────────────────────────────

/// A clonable sender handle backed by a `tokio::sync::mpsc::Sender`.
pub struct TokioSender<M: Send + 'static> {
    pub(crate) inner: Arc<mpsc::Sender<Envelope<M>>>,
}

impl<M: Send + 'static> Clone for TokioSender<M> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

unsafe impl<M: Send + 'static> Send for TokioSender<M> {}
unsafe impl<M: Send + 'static> Sync for TokioSender<M> {}

// ── TokioStream ───────────────────────────────────────────────────────────────

/// The receiver half plus a `Stream` adapter.
///
/// The [`Stream`] implementation on this type **never yields `None`** by design:
/// if the channel is closed it panics, matching the invariant that channels are
/// never intentionally closed in the bloxide actor model.
pub struct TokioStream<M: Send + 'static> {
    pub(crate) inner: mpsc::Receiver<Envelope<M>>,
}

impl<M: Send + 'static> Unpin for TokioStream<M> {}
unsafe impl<M: Send + 'static> Send for TokioStream<M> {}

impl<M: Send + 'static> Stream for TokioStream<M> {
    type Item = Envelope<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Propagate None so that callers (timer service, supervision loop) can
        // detect channel close and shut down gracefully.
        self.inner.poll_recv(cx)
    }
}

// ── Error types ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct TokioSendError;

#[derive(Debug)]
pub struct TokioTrySendError;
