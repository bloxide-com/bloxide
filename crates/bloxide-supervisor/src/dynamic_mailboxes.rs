// Copyright 2025 Bloxide, all rights reserved
//! Custom `Mailboxes` implementation for the dynamic supervisor.
//!
//! When the `dynamic` feature is enabled, the supervisor receives three
//! stream types:
//!   1. `ChildLifecycleEvent` — child lifecycle observations
//!   2. `SupervisorControl<R>` — control commands
//!   3. `F::Request` — spawn requests (typed by the application's `SpawnFactory`)
//!
//! The generated tuple `Mailboxes` blanket impl requires `E: From<stream::Item>`
//! for each stream. For the spawn stream this means we need
//! `From<Envelope<F::Request>>` on `SupervisorEvent`, but Rust's coherence
//! checker (E0119) rejects it because it cannot prove `F::Request` is never
//! `ChildLifecycleEvent`.
//!
//! This custom struct avoids the `From` requirement entirely by mapping
//! stream items to `SupervisorEvent` variants directly in `poll_next()`.

#![cfg(feature = "dynamic")]

use core::pin::Pin;
use core::task::{Context, Poll};

use bloxide_core::capability::BloxRuntime;
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::mailboxes::Mailboxes;
use bloxide_core::messaging::Envelope;
use bloxide_supervisor_context::control::SupervisorControl;
use bloxide_supervisor_context::event::SupervisorEvent;
use bloxide_supervisor_context::spawn::SpawnFactory;
use futures_core::Stream;

/// A mailbox set for the dynamic supervisor that holds three typed streams
/// and maps their `Envelope<M>` items into `SupervisorEvent<R, F>` variants.
///
/// # Type parameters
///
/// * `R` — the runtime type from the `SupervisorSpec<R, F>` (used for
///   `SupervisorControl<R>` and `F::Request` via `F: SpawnFactory<R>`).
/// * `Rt` — the runtime type at the wiring site (used for `Rt::Stream<M>`).
///   In practice `Rt = R`, but the `MachineSpec::Mailboxes<Rt>` GAT requires
///   generality.
/// * `F` — the `SpawnFactory<R>` that defines the `Request` type.
///
/// Polling priority order (highest first):
///   1. `child_rx` — child lifecycle events
///   2. `control_rx` — supervisor control commands
///   3. `spawn_rx` — spawn requests
///
/// Like the generated tuple impls, `Poll::Ready(None)` from any stream
/// (channel closed) propagates immediately as `Poll::Ready(None)` for the
/// whole mailbox set, signalling graceful shutdown.
pub struct SupervisorMailboxes<R: BloxRuntime, Rt: BloxRuntime, F: SpawnFactory<R>> {
    pub child_rx: Rt::Stream<ChildLifecycleEvent>,
    pub control_rx: Rt::Stream<SupervisorControl<R>>,
    pub spawn_rx: Rt::Stream<F::Request>,
}

impl<R, Rt, F> Mailboxes<SupervisorEvent<R, F>> for SupervisorMailboxes<R, Rt, F>
where
    R: BloxRuntime,
    Rt: BloxRuntime,
    F: SpawnFactory<R> + 'static,
{
    fn poll_next(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<SupervisorEvent<R, F>>> {
        // Poll child_rx first (highest priority).
        match Pin::new(&mut self.child_rx).poll_next(cx) {
            Poll::Ready(Some(Envelope(_, msg))) => {
                return Poll::Ready(Some(SupervisorEvent::Child(msg)));
            }
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Pending => {}
        }

        // Then control_rx.
        match Pin::new(&mut self.control_rx).poll_next(cx) {
            Poll::Ready(Some(Envelope(_, msg))) => {
                return Poll::Ready(Some(SupervisorEvent::Control(msg)));
            }
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Pending => {}
        }

        // Then spawn_rx.
        match Pin::new(&mut self.spawn_rx).poll_next(cx) {
            Poll::Ready(Some(Envelope(_, msg))) => {
                return Poll::Ready(Some(SupervisorEvent::Spawn(msg)));
            }
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Pending => {}
        }

        // All streams pending.
        Poll::Pending
    }
}

// The `Mailboxes` trait requires `Send + 'static + Unpin`.
// `Rt::Stream<M>` is already `Unpin + Send + 'static` per the `BloxRuntime`
// trait definition. Since all three fields are `Rt::Stream<M>` variants
// (which are `Send + Unpin + 'static`), the struct satisfies these bounds
// automatically. `F` does not appear as a field type — only `F::Request`
// does, which is `Send + 'static` from the `SpawnFactory` trait.
