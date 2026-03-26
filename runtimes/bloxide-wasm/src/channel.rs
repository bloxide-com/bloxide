// Copyright 2025 Bloxide, all rights reserved
use alloc::boxed::Box;
use core::pin::Pin;
use core::task::{Context, Poll};

use bloxide_core::messaging::Envelope;
use futures_core::Stream;

// ── WasmSender ────────────────────────────────────────────────────────────────

/// Cloneable sender backed by a bounded `async_channel`.
pub struct WasmSender<M: Send + 'static> {
    pub(crate) inner: async_channel::Sender<Envelope<M>>,
}

impl<M: Send + 'static> Clone for WasmSender<M> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

unsafe impl<M: Send + 'static> Send for WasmSender<M> {}
unsafe impl<M: Send + 'static> Sync for WasmSender<M> {}

// ── WasmStream ────────────────────────────────────────────────────────────────

/// Receiver side as a pinned [`Stream`] of envelopes (`async_channel::Receiver` is `!Unpin`).
pub struct WasmStream<M: Send + 'static> {
    pub(crate) inner: Pin<Box<async_channel::Receiver<Envelope<M>>>>,
}

impl<M: Send + 'static> Unpin for WasmStream<M> {}

unsafe impl<M: Send + 'static> Send for WasmStream<M> {}

impl<M: Send + 'static> Stream for WasmStream<M> {
    type Item = Envelope<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

// ── Error types ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct WasmSendError;

#[derive(Debug)]
pub struct WasmTrySendError;
