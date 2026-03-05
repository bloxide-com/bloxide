// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::messaging::Envelope;
use core::pin::Pin;
use core::task::{Context, Poll};
use embassy_sync::channel::{DynamicReceiver, DynamicSender};
use futures_core::Stream;

// ── EmbassySender ─────────────────────────────────────────────────────────────

/// A clonable sender handle backed by an embassy-sync `DynamicSender`.
pub struct EmbassySender<M: Send + 'static> {
    pub(crate) inner: DynamicSender<'static, Envelope<M>>,
}

impl<M: Send + 'static> Clone for EmbassySender<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: Send + 'static> Copy for EmbassySender<M> {}

// SAFETY: EmbassySender can only be constructed via StaticChannelCap::channel(),
// which hardcodes CriticalSectionRawMutex on the underlying Channel. The inner
// DynamicSender holds a &'static reference to that channel, and
// CriticalSectionRawMutex provides mutual exclusion via interrupt masking.
unsafe impl<M: Send + 'static> Send for EmbassySender<M> {}
unsafe impl<M: Send + 'static> Sync for EmbassySender<M> {}

// ── EmbassyStream ─────────────────────────────────────────────────────────────

/// The receiver half plus a `Stream` adapter.
///
/// The [`Stream`] implementation on this type **never yields `None`** —
/// `poll_next` always returns `Poll::Ready(Some(...))` or `Poll::Pending`.
/// This is an intentional design constraint matching Embassy's channel
/// semantics: channels have `'static` lifetime and are never closed.
/// Callers should not rely on stream termination as a shutdown signal.
pub struct EmbassyStream<M: Send + 'static> {
    pub(crate) inner: DynamicReceiver<'static, Envelope<M>>,
}

impl<M: Send + 'static> Unpin for EmbassyStream<M> {}

// SAFETY: Same reasoning as EmbassySender — the inner DynamicReceiver holds a
// &'static reference to a Channel<CriticalSectionRawMutex, ..> created by
// StaticChannelCap::channel().
unsafe impl<M: Send + 'static> Send for EmbassyStream<M> {}

impl<M: Send + 'static> Stream for EmbassyStream<M> {
    type Item = Envelope<M>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // poll_receive registers the waker with Embassy's internal WakerRegistration
        // so this task sleeps until a message arrives, rather than busy-polling.
        self.inner.poll_receive(cx).map(Some)
    }
}

// ── Error types ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct EmbassyTrySendError;
