// Copyright 2025 Bloxide, all rights reserved
//! Generic child group builder — creates channels and assembles a `ChildGroup`.
//!
//! This builder is generic over the runtime `R` and the control message type `Ctrl`.
//! The runtime provides group channels via `GroupChannelCap`; the app specifies
//! the control message type (e.g. `ChildCtrl<R>` if using the supervisor).
//!
//! Runtimes do NOT need to know about `ChildCtrl` — the app chooses `Ctrl`.

use crate::{ChildGroup, ChildPolicy, GroupShutdown};
use bloxide_core::{
    capability::GroupChannelCap,
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::{ActorId, ActorRef},
};

/// Builder for assembling a `ChildGroup` with group channels.
///
/// Generic over runtime `R` (must support `GroupChannelCap`) and control message
/// type `Ctrl` (chosen by the app — e.g. `ChildCtrl<R>`). One builder serves
/// every runtime: dynamic runtimes (Tokio, TestRuntime) create runtime-capacity
/// channels, static runtimes (Embassy) create const-capacity channels — so
/// generated wiring is identical across runtimes.
///
/// Channel capacities are const generics with defaults (notify 32, control 16,
/// per-child lifecycle 4). Generated wiring overrides them from the system.toml
/// `[supervision.capacities]` table, computing defaults from the child count —
/// the channels carry the supervision control plane, so they are sized to make
/// "full" a bug rather than routine backpressure (confirm-before-record still
/// handles Full correctly when it happens).
///
/// Created with `::new(shutdown)`, children are added via `add_child()`, and the
/// group is consumed via `finish()`.
pub struct ChildGroupBuilder<
    R: GroupChannelCap,
    Ctrl: Send + 'static,
    const NOTIFY: usize = 32,
    const CONTROL: usize = 16,
    const LIFECYCLE: usize = 4,
> {
    group: ChildGroup<R>,
    notify_ref: ActorRef<ChildLifecycleEvent, R>,
    notify_rx: Option<R::Receiver<ChildLifecycleEvent>>,
    control_ref: ActorRef<Ctrl, R>,
    control_rx: Option<R::Receiver<Ctrl>>,
}

impl<R, Ctrl, const NOTIFY: usize, const CONTROL: usize, const LIFECYCLE: usize>
    ChildGroupBuilder<R, Ctrl, NOTIFY, CONTROL, LIFECYCLE>
where
    R: GroupChannelCap,
    Ctrl: Send + 'static,
{
    /// Create a new builder with the given group shutdown policy.
    ///
    /// Allocates notify and control channels. The notify channel receives
    /// `ChildLifecycleEvent` from child actors; the control channel receives
    /// `Ctrl` messages (e.g. `RegisterChild`, `RegisterDynamicChild`).
    pub fn new(shutdown: GroupShutdown, max_misses: u8) -> Self {
        let notify_id = R::alloc_group_id();
        let (notify_ref, notify_rx) = R::group_channel::<ChildLifecycleEvent, NOTIFY>(notify_id);

        let control_id = R::alloc_group_id();
        let (control_ref, control_rx) = R::group_channel::<Ctrl, CONTROL>(control_id);

        Self {
            group: ChildGroup::new(shutdown, max_misses),
            notify_ref,
            notify_rx: Some(notify_rx),
            control_ref,
            control_rx: Some(control_rx),
        }
    }

    /// Add a child to the group with the given policy.
    ///
    /// Creates a per-child lifecycle channel and registers the child.
    /// Returns the lifecycle receive stream and the notify sender.
    ///
    /// # Panics
    ///
    /// Panics if the policy is invalid for a static child (`Abort`/`Kill` —
    /// they need handles) or the id is already registered. This is boot-time
    /// wiring code: generated apps only ever emit valid policies (the
    /// system.toml vocabulary has no abort/kill), so a panic here is a
    /// hand-written wiring bug caught at startup, not a message-driven event.
    pub fn add_child(
        &mut self,
        id: ActorId,
        policy: ChildPolicy,
    ) -> (
        R::Receiver<LifecycleCommand>,
        R::Sender<ChildLifecycleEvent>,
    ) {
        let (lifecycle_ref, cmd_rx) = R::group_channel::<LifecycleCommand, LIFECYCLE>(id);
        self.group.try_add(id, lifecycle_ref, policy).expect(
            "ChildGroupBuilder::add_child — invalid policy or duplicate id in static wiring",
        );
        (cmd_rx, self.notify_ref.sender())
    }

    /// Get the control channel sender (for registering children externally).
    pub fn control_ref(&self) -> ActorRef<Ctrl, R> {
        self.control_ref.clone()
    }

    /// Get the notify channel sender (for children to report lifecycle events).
    pub fn notify_sender(&self) -> R::Sender<ChildLifecycleEvent> {
        self.notify_ref.sender()
    }

    /// Get the notify channel reference (for wiring to the managing blox).
    pub fn notify_ref(&self) -> ActorRef<ChildLifecycleEvent, R> {
        self.notify_ref.clone()
    }

    /// Consume the builder and return the assembled group plus channel receivers.
    ///
    /// Returns `(ChildGroup, notify_rx, control_rx)`.
    pub fn finish(
        self,
    ) -> (
        ChildGroup<R>,
        R::Receiver<ChildLifecycleEvent>,
        R::Receiver<Ctrl>,
    ) {
        (
            self.group,
            self.notify_rx.expect("notify_rx already taken"),
            self.control_rx.expect("control_rx already taken"),
        )
    }
}
