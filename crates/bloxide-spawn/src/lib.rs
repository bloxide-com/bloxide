// Copyright 2025 Bloxide, all rights reserved
#![no_std]
//! Spawn capability for bloxide — `SpawnCap`, `Kill`, and the `spawn_child` helper.
//!
//! This crate is a platform primitive: the ability to spawn actor tasks at
//! runtime. Runtimes that support dynamic spawning (Tokio) implement
//! `SpawnCap` and use `Kill` as their `KillCapability`. Runtimes that don't
//! (Embassy) use `NoKill` from `bloxide-core` and never depend on this crate.
//!
//! The `spawn_child` helper and `ChildRegistrar` trait let any managing blox
//! (supervisor or custom) register spawned children without depending on
//! the supervisor.

use bloxide_core::capability::{BloxRuntime, DynamicChannelCap};
use bloxide_core::child_management::{AbortCommand, ChildPolicy};
use bloxide_core::lifecycle::{ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::{ActorId, ActorRef};

use core::fmt;
use core::future::Future;

// Re-export KillCapability and NoKill so downstream crates can get everything
// from one place.
pub use bloxide_core::capability::{KillCapability, NoKill};

/// Tier 2 capability for runtimes that support spawning actor tasks at runtime.
///
/// Extends `DynamicChannelCap` (which provides `alloc_actor_id` and `channel`).
/// Blox crates that need dynamic spawning declare `R: SpawnCap`.
/// Embassy does NOT implement this trait — use static wiring for Embassy.
///
/// The associated `TaskHandle` type is returned by `spawn` and is used to
/// produce a `KillHandle` (the cloneable ripcord). For Tokio,
/// `TaskHandle = JoinHandle<()>` and `KillHandle = tokio::task::AbortHandle`.
/// For a future Embassy task-pool runtime, `KillHandle` would be `()` (no
/// external kill — the kill mailbox is sufficient) or whatever Embassy
/// provides if [issue #3197](https://github.com/embassy-rs/embassy/issues/3197)
/// is implemented.
///
/// All types are concrete, by-value — no `Arc<dyn>`, no dynamic dispatch.
pub trait SpawnCap: DynamicChannelCap {
    /// Handle to a spawned task. Used to derive a [`KillHandle`](Self::KillHandle).
    /// Consumed by [`kill_handle`](Self::kill_handle).
    type TaskHandle: Send + 'static;

    /// Cloneable handle for external task kill. Must be `Clone` so it can
    /// be extracted from `&Event` in action functions (the HSM engine passes
    /// `&Event`, not `&mut Event`). `()` for runtimes without external kill.
    type KillHandle: Clone + Send + 'static;

    /// Spawn a future as an independent task and return a handle.
    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self::TaskHandle;

    /// Derive a cloneable kill handle from a task handle.
    /// The task handle is consumed; the task continues running (drop does not kill).
    fn kill_handle(handle: Self::TaskHandle) -> Self::KillHandle;

    /// Kill a spawned task immediately via its kill handle. No callbacks fire —
    /// the task is dropped in-place. The handle is consumed and cannot be reused.
    fn kill(handle: Self::KillHandle);
}

/// Kill capability via `SpawnCap::kill`. Used by dynamic runtimes (Tokio).
///
/// This lives in `bloxide-spawn` (not `bloxide-core`) because it requires the
/// `SpawnCap` bound — only runtimes that can spawn can use this. Static
/// runtimes (Embassy) use `NoKill` from `bloxide-core` instead.
pub struct Kill;

impl<R: BloxRuntime + SpawnCap> KillCapability<R> for Kill {
    type Handle = R::KillHandle;
    fn kill(handle: R::KillHandle) {
        R::kill(handle);
    }
}

/// What a spawn function returns — the lifecycle and capability refs needed
/// to register the child with whatever blox manages it.
///
/// This type is NOT app-specific and NOT supervisor-specific. It carries only
/// lifecycle types and capability mailbox refs. The app-specific handles
/// (domain_ref, ctrl_ref, etc.) go back to the requester via the spawn
/// request's reply-to channel, not through here.
///
/// The `kill_handle` IS here — the spawn function gets a `TaskHandle` from
/// `R::spawn()`, converts it to a cloneable `KillHandle` via
/// `R::kill_handle()`, and passes it here so the managing blox can call
/// `R::Kill::kill(handle)` as the ripcord for unresponsive children. For
/// `NoKill` runtimes this is `()`.
pub struct SpawnOutput<R: BloxRuntime> {
    /// The allocated actor ID for the new child.
    pub child_id: ActorId,
    /// Channel for sending lifecycle commands (Start, Stop, Reset).
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    /// Abort capability mailbox (send side). The managing blox sends
    /// `AbortCommand` here; the child's task receives it and self-terminates
    /// cooperatively (no callbacks, no dispatch).
    pub abort_ref: ActorRef<AbortCommand, R>,
    /// Cloneable kill handle for external task kill (the ripcord). The
    /// managing blox calls `R::Kill::kill(handle)` when the child is
    /// unresponsive and `ChildPolicy::Kill` fires. `()` for `NoKill` runtimes,
    /// `R::KillHandle` for `Kill` runtimes. Must be `Clone` so action
    /// functions can extract it from `&Event` (the HSM engine passes `&Event`,
    /// not `&mut Event`).
    pub kill_handle: <R::Kill as KillCapability<R>>::Handle,
    /// Supervision policy for this child.
    pub policy: ChildPolicy,
}

impl<R: BloxRuntime> Clone for SpawnOutput<R> {
    fn clone(&self) -> Self {
        Self {
            child_id: self.child_id,
            lifecycle_ref: self.lifecycle_ref.clone(),
            abort_ref: self.abort_ref.clone(),
            kill_handle: self.kill_handle.clone(),
            policy: self.policy,
        }
    }
}

impl<R: BloxRuntime> fmt::Debug for SpawnOutput<R>
where
    <R::Kill as KillCapability<R>>::Handle: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpawnOutput")
            .field("child_id", &self.child_id)
            .field("policy", &self.policy)
            .finish()
    }
}

/// A spawn function creates a child actor and returns the handles the
/// supervisor needs for lifecycle management and capability control.
///
/// This is a `fn` pointer, not a trait. The application provides the
/// concrete function at wiring time. The function is stateless — all
/// per-request state comes through the request parameter.
///
/// The `Req` type parameter is the application's concrete spawn request
/// enum (e.g., `SpawnRequest<R>` in pool-messages). The runtime helper
/// is generic over `Req` so it doesn't depend on any specific app's
/// messages crate.
pub type SpawnFn<R, Req> = fn(req: Req, notify: ActorRef<ChildLifecycleEvent, R>) -> SpawnOutput<R>;

/// A blox that manages spawned children implements this to define how
/// `SpawnOutput` is wrapped into its own control-plane message type.
///
/// The associated `RegisterMsg` is the message type the spawn helper sends
/// on the managing blox's control mailbox after a child is spawned.
///
/// Our standard supervisor implements this with `RegisterMsg = SupervisorControl<R>`.
/// A user's custom blox implements it with their own message type.
pub trait ChildRegistrar<R: BloxRuntime> {
    /// The control-plane message type that carries a `SpawnOutput` to the
    /// managing blox. Sent on the managing blox's control mailbox.
    type RegisterMsg: Send + 'static;

    /// Wrap a `SpawnOutput` into the managing blox's registration message.
    fn register(output: SpawnOutput<R>) -> Self::RegisterMsg;
}

/// Spawn a child actor and register it with the managing blox.
///
/// Called by the requesting blox (e.g., the Pool) — NOT by the supervisor.
/// The requesting blox provides the spawn function and the request.
///
/// This helper:
///   1. Calls the spawn function to create the child (channels, context, task)
///   2. Sends the registration message (typed by `C::RegisterMsg`) to the
///      managing blox's control mailbox
///
/// The managing blox receives the registration message and starts managing the
/// child's lifecycle. The managing blox never sees the request type.
///
/// # Type Parameters
///
/// - `R` — the runtime (must support `SpawnCap` + `DynamicChannelCap`)
/// - `Req` — the application's concrete spawn request type
/// - `C` — the `ChildRegistrar` implementation. Determines how `SpawnOutput`
///   is wrapped into the managing blox's control-plane message.
pub fn spawn_child<R, Req, C>(
    spawn_fn: SpawnFn<R, Req>,
    req: Req,
    control_ref: &ActorRef<C::RegisterMsg, R>,
    notify_ref: &ActorRef<ChildLifecycleEvent, R>,
    from: ActorId,
) -> Result<(), R::TrySendError>
where
    R: BloxRuntime,
    Req: Send + Clone + 'static,
    C: ChildRegistrar<R>,
{
    // 1. Call the spawn function — creates channels, constructs child, spawns task
    let output: SpawnOutput<R> = spawn_fn(req, notify_ref.clone());

    // 2. Wrap output into the managing blox's registration message and send it
    let msg = C::register(output);
    control_ref.try_send(from, msg)?;

    Ok(())
}
