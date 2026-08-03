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
/// Created with `::new(shutdown)`, children are added via `add_child()`, and the
/// group is consumed via `finish()`.
pub struct ChildGroupBuilder<R: GroupChannelCap, Ctrl: Send + 'static> {
    group: ChildGroup<R>,
    notify_ref: ActorRef<ChildLifecycleEvent, R>,
    notify_rx: Option<R::Receiver<ChildLifecycleEvent>>,
    control_ref: ActorRef<Ctrl, R>,
    control_rx: Option<R::Receiver<Ctrl>>,
}

impl<R, Ctrl> ChildGroupBuilder<R, Ctrl>
where
    R: GroupChannelCap,
    Ctrl: Send + 'static,
{
    /// Create a new builder with the given group shutdown policy.
    ///
    /// Allocates notify and control channels. The notify channel receives
    /// `ChildLifecycleEvent` from child actors; the control channel receives
    /// `Ctrl` messages (e.g. `RegisterChild`, `RegisterDynamicChild`).
    pub fn new(shutdown: GroupShutdown) -> Self {
        let notify_id = R::alloc_group_id();
        let (notify_ref, notify_rx) = R::group_channel::<ChildLifecycleEvent, 32>(notify_id);

        let control_id = R::alloc_group_id();
        let (control_ref, control_rx) = R::group_channel::<Ctrl, 16>(control_id);

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
        let (lifecycle_ref, cmd_rx) = R::group_channel::<LifecycleCommand, 4>(id);
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
