// Copyright 2025 Bloxide, all rights reserved
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

// SAFETY: TokioSender wraps an Arc<mpsc::Sender<Envelope<M>>>, which is
// already Send + Sync when M: Send (Envelope is a plain data tuple struct).
// The manual impls therefore weaken no invariant.
unsafe impl<M: Send + 'static> Send for TokioSender<M> {}
unsafe impl<M: Send + 'static> Sync for TokioSender<M> {}

// ── TokioStream ───────────────────────────────────────────────────────────────

/// The receiver half plus a `Stream` adapter.
///
/// The [`Stream`] implementation propagates `None` when the channel closes,
/// so callers (timer service, supervision loop) can detect channel close and
/// shut down gracefully.
pub struct TokioStream<M: Send + 'static> {
    pub(crate) inner: mpsc::Receiver<Envelope<M>>,
}

impl<M: Send + 'static> Unpin for TokioStream<M> {}

// SAFETY: TokioStream wraps an mpsc::Receiver<Envelope<M>>, which is
// already Send when M: Send. The manual impl weakens no invariant.
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

/// Error returned by a non-blocking send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokioTrySendError {
    /// Channel buffer is full (backpressure).
    Full,
    /// Channel is closed — the receiving task is gone.
    Closed,
}
