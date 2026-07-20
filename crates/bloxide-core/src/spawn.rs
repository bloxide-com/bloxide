// Copyright 2025 Bloxide, all rights reserved
//! Spawn types — `SpawnOutput`, `ChildRegistrar`, `SpawnFn`, `spawn_child` helper.
//!
//! These types live in `bloxide-core` because spawning is NOT supervisor-specific.
//! Any blox that manages children needs the same lifecycle refs, abort ref, task
//! handle, and policy from a spawn operation.

use crate::capability::{BloxRuntime, KillCapability};
use crate::child_management::{AbortCommand, ChildPolicy};
use crate::lifecycle::ChildLifecycleEvent;
use crate::lifecycle::LifecycleCommand;
use crate::messaging::{ActorId, ActorRef};

use core::fmt;

/// What a spawn function returns — the lifecycle and capability refs needed
/// to register the child with whatever blox manages it.
///
/// This type is NOT app-specific and NOT supervisor-specific. It carries only
/// lifecycle types and capability mailbox refs. The app-specific handles
/// (domain_ref, ctrl_ref, etc.) go back to the requester via the spawn
/// request's reply-to channel, not through here.
///
/// The `abort_handle` IS here — the spawn function gets a `TaskHandle` from
/// `R::spawn()`, converts it to a cloneable `AbortHandle` via
/// `R::abort_handle()`, and passes it here so the managing blox can call
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
    /// Cloneable abort handle for external task kill (the ripcord). The
    /// managing blox calls `R::Kill::kill(handle)` when the child is
    /// unresponsive and `ChildPolicy::Kill` fires. `()` for `NoKill` runtimes,
    /// `R::AbortHandle` for `Kill` runtimes. Must be `Clone` so action
    /// functions can extract it from `&Event` (the HSM engine passes `&Event`,
    /// not `&mut Event`).
    pub abort_handle: <R::Kill as KillCapability<R>>::Handle,
    /// Supervision policy for this child.
    pub policy: ChildPolicy,
}

impl<R: BloxRuntime> Clone for SpawnOutput<R> {
    fn clone(&self) -> Self {
        Self {
            child_id: self.child_id,
            lifecycle_ref: self.lifecycle_ref.clone(),
            abort_ref: self.abort_ref.clone(),
            abort_handle: self.abort_handle.clone(),
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

/// Spawn a supervised child actor.
///
/// Called by the requesting blox (e.g., the Pool) — NOT by the supervisor.
/// The requesting blox provides the spawn function and the request.
///
/// This helper:
///   1. Calls the spawn function to create the child (channels, context, task)
///   2. Sends the registration message (typed by `C::RegisterMsg`) to the
///      managing blox's control mailbox
///
/// The supervisor receives the registration message and starts managing the
/// child's lifecycle. The supervisor never sees the request type.
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
