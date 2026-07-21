// Copyright 2025 Bloxide, all rights reserved
//! Generic child group builder — creates channels and assembles a `ChildGroup`.
//!
//! This builder is generic over the runtime `R` and the control message type `Ctrl`.
//! The runtime provides channel primitives via `DynamicChannelCap`; the app specifies
//! the control message type (e.g. `SupervisorControl<R>` if using the supervisor).
//!
//! Runtimes do NOT need to know about `SupervisorControl` — the app chooses `Ctrl`.

use crate::{ChildGroup, ChildPolicy, GroupShutdown};
use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::{ActorId, ActorRef},
};

/// Builder for assembling a `ChildGroup` with dynamic channels.
///
/// Generic over runtime `R` (must support `DynamicChannelCap`) and control message
/// type `Ctrl` (chosen by the app — e.g. `SupervisorControl<R>`).
///
/// Created with `::new(shutdown)`, children are added via `add_child()`, and the
/// group is consumed via `finish()`.
pub struct ChildGroupBuilder<R: BloxRuntime, Ctrl: Send + 'static> {
    group: ChildGroup<R>,
    notify_ref: ActorRef<ChildLifecycleEvent, R>,
    notify_rx: Option<R::Receiver<ChildLifecycleEvent>>,
    control_ref: ActorRef<Ctrl, R>,
    control_rx: Option<R::Receiver<Ctrl>>,
}

impl<R, Ctrl> ChildGroupBuilder<R, Ctrl>
where
    R: BloxRuntime + DynamicChannelCap,
    Ctrl: Send + 'static,
{
    /// Create a new builder with the given group shutdown policy.
    ///
    /// Allocates notify and control channels. The notify channel receives
    /// `ChildLifecycleEvent` from child actors; the control channel receives
    /// `Ctrl` messages (e.g. `RegisterChild`, `RegisterDynamicChild`).
    pub fn new(shutdown: GroupShutdown) -> Self {
        let notify_id = R::alloc_actor_id();
        let (notify_ref, notify_rx) =
            R::channel::<ChildLifecycleEvent>(notify_id, 32);

        let control_id = R::alloc_actor_id();
        let (control_ref, control_rx) = R::channel::<Ctrl>(control_id, 16);

        Self {
            group: ChildGroup::new(shutdown),
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
    pub fn add_child(
        &mut self,
        id: ActorId,
        policy: ChildPolicy,
    ) -> (
        R::Receiver<LifecycleCommand>,
        R::Sender<ChildLifecycleEvent>,
    ) {
        let (lifecycle_ref, cmd_rx) = R::channel::<LifecycleCommand>(id, 4);
        self.group.add(id, lifecycle_ref, policy);
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
