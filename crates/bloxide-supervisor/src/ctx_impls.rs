// Copyright 2025 Bloxide, all rights reserved
//! Hand-written trait implementations for the generated `SupervisorCtx`.
//!
//! The generated `ctx.rs` derives `BloxCtx` which auto-generates `HasSelfId`
//! and the `new()` constructor. However, `HasChildGroup`, `HasChildGroupMut`,
//! `HasPending`, and `HasChildNotify` are not auto-detected (the `children`
//! and `child_notify` fields are `#[blox_ctx(skip)]`, and `pending` is a
//! state field). We implement them manually here.
//!
//! With the `dynamic` feature, `SupervisorCtx` has two type parameters
//! `<R, F>` and also needs `HasSpawnFactory` implemented.

use bloxide_core::capability::BloxRuntime;
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::messaging::ActorRef;
use bloxide_supervisor_context::{
    ChildAction, ChildGroup, HasChildGroup, HasChildGroupMut, HasChildNotify, HasPending,
};

#[cfg(feature = "dynamic")]
use bloxide_supervisor_context::{HasSpawnFactory, SpawnFactory};

use crate::generated::ctx::SupervisorCtx;

// ── Non-dynamic trait impls ─────────────────────────────────────────────────

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> HasChildGroup<R> for SupervisorCtx<R> {
    fn children(&self) -> &ChildGroup<R> {
        &self.children
    }
}

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> HasChildGroupMut<R> for SupervisorCtx<R> {
    fn children_mut(&mut self) -> &mut ChildGroup<R> {
        &mut self.children
    }
}

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> HasPending for SupervisorCtx<R> {
    fn pending(&self) -> ChildAction {
        self.pending
    }

    fn set_pending(&mut self, action: ChildAction) {
        self.pending = action;
    }
}

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> HasChildNotify<R> for SupervisorCtx<R> {
    fn child_notify(&self) -> &ActorRef<ChildLifecycleEvent, R> {
        &self.child_notify
    }
}

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> SupervisorCtx<R> {
    /// Returns `true` when every child in the group has stopped.
    ///
    /// Used by the `ShuttingDown` state's transition guard to decide
    /// when to reset the supervisor.
    pub fn all_children_stopped(&self) -> bool {
        self.children.all_stopped()
    }
}

// ── Dynamic trait impls ─────────────────────────────────────────────────────

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> HasChildGroup<R> for SupervisorCtx<R, F> {
    fn children(&self) -> &ChildGroup<R> {
        &self.children
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> HasChildGroupMut<R> for SupervisorCtx<R, F> {
    fn children_mut(&mut self) -> &mut ChildGroup<R> {
        &mut self.children
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> HasPending for SupervisorCtx<R, F> {
    fn pending(&self) -> ChildAction {
        self.pending
    }

    fn set_pending(&mut self, action: ChildAction) {
        self.pending = action;
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> HasChildNotify<R> for SupervisorCtx<R, F> {
    fn child_notify(&self) -> &ActorRef<ChildLifecycleEvent, R> {
        &self.child_notify
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> HasSpawnFactory<R> for SupervisorCtx<R, F> {
    type Factory = F;
    fn spawn_factory(&self) -> &F {
        &self.spawn_factory
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> SupervisorCtx<R, F> {
    /// Returns `true` when every child in the group has stopped.
    pub fn all_children_stopped(&self) -> bool {
        self.children.all_stopped()
    }
}
