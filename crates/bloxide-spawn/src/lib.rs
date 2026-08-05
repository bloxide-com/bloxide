// Copyright 2025 Bloxide, all rights reserved
#![no_std]
//! Spawn capability for bloxide — `SpawnCap`, `Kill`, the platform spawn API
//! (`ActorParts` + `spawn_actor_task`), and the `spawn_dynamic_child` helper.
//!
//! This crate is a platform primitive: the ability to spawn actor tasks at
//! runtime. Runtimes that support dynamic spawning (Tokio) implement
//! `SpawnCap` and use `Kill` as their `KillCapability`. Runtimes that don't
//! (Embassy) use `NoKill` from `bloxide-core` and never depend on this crate.
//!
//! Dynamic spawning is a two-layer composition:
//!
//! - **Domain factories** (impl crates) build an [`ActorParts`] — machine,
//!   mailboxes, lifecycle/abort channels, policy — and never touch `run()`,
//!   `RunConfig`, or `SpawnCap`.
//! - **The platform spawn** ([`spawn_actor_task`]) consumes the parts and
//!   does all executor mechanics: run-loop configuration, task spawn, and
//!   kill-handle derivation.
//!
//! The `spawn_dynamic_child` helper and `ChildRegistrar` trait let any managing blox
//! (supervisor or custom) register spawned children without depending on
//! the supervisor.

use bloxide_child_management::ChildPolicy;
use bloxide_core::capability::{BloxRuntime, DynamicChannelCap};
use bloxide_core::engine::StateMachine;
use bloxide_core::lifecycle::AbortCommand;
use bloxide_core::lifecycle::{ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::{ActorId, ActorRef};
use bloxide_core::runloop::{run, RunConfig};
use bloxide_core::spec::MachineSpec;

use core::fmt;
use core::future::Future;

// Re-export KillCapability and NoKill so downstream crates can get everything
// from one place.
pub use bloxide_core::capability::{KillCapability, NoKill};

/// Tier 2 capability for runtimes that support spawning actor tasks at runtime.
///
/// Extends `DynamicChannelCap` (which provides `alloc_actor_id` and `channel`).
/// This is a Tier 2 runtime-facing trait — blox crates never declare `R: SpawnCap`
/// directly. Blox crates that need dynamic spawning use factory injection
/// (see AGENTS.md invariant #15 and spec doc 13). The trait is implemented by
/// runtimes (Tokio) and consumed by the spawn infrastructure and wiring layer.
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
    const CAN_KILL: bool = true;
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
/// enum (e.g., `SpawnRequest<R>` in the pool demo's `blox-ctx-pool-ref`).
/// The runtime helper is generic over `Req` so it doesn't depend on any
/// specific app's domain crate.
pub type SpawnFn<R, Req> = fn(req: Req, notify: ActorRef<ChildLifecycleEvent, R>) -> SpawnOutput<R>;

/// Everything a domain factory builds **before** the platform spawns the
/// actor task — the "build" half of the dynamic-spawn composition.
///
/// A domain factory (impl crate) assembles these parts from its spawn
/// request: it allocates the child id, creates the domain and
/// lifecycle/abort channels, and constructs the state machine. It returns
/// `ActorParts` and **never touches `run()`, `RunConfig`, or `SpawnCap`** —
/// all executor mechanics are owned by [`spawn_actor_task`]. This keeps impl
/// crates free of run-loop plumbing, so the same factory code works
/// unchanged across runtimes and system codegen can emit the composition
/// mechanically at the wiring layer:
///
/// ```text
/// |req, notify| spawn_actor_task(build_parts(req), notify)   // a SpawnFn<R, Req>
/// ```
///
/// The send-side domain refs are NOT here — they are app-specific handles
/// that go back to the requester via the spawn request's reply channel,
/// exactly as with [`SpawnOutput`]'s app-specific handles. Only what the
/// platform spawn needs is carried.
///
/// All fields are concrete, by-value — no `Arc<dyn>`, no dynamic dispatch.
pub struct ActorParts<S: MachineSpec, R: BloxRuntime> {
    /// The allocated actor ID for the new child.
    pub child_id: ActorId,
    /// The child's state machine, constructed in implicit Init (silent — no
    /// callbacks fire until the supervisor sends Start).
    pub machine: StateMachine<S>,
    /// The child's domain mailboxes (receive side), moved into the run loop.
    pub mailboxes: S::Mailboxes<R>,
    /// Channel for sending lifecycle commands (Start, Stop, Reset) —
    /// returned via [`SpawnOutput`] so the managing blox can drive the child.
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    /// Lifecycle command stream (receive side) — moved into the run loop.
    pub lifecycle_rx: R::Stream<LifecycleCommand>,
    /// Abort capability mailbox (send side) — returned via [`SpawnOutput`] so
    /// the managing blox can trigger cooperative self-termination.
    pub abort_ref: ActorRef<AbortCommand, R>,
    /// Abort command stream (receive side) — moved into the run loop.
    pub abort_rx: R::Stream<AbortCommand>,
    /// Supervision policy the managing blox applies to this child.
    pub policy: ChildPolicy,
}

/// The platform spawn — the "executor mechanics" half of the dynamic-spawn
/// composition. Consumes the [`ActorParts`] a domain factory built, spawns
/// the run loop as a task via [`SpawnCap`], and returns the handles the
/// managing blox needs.
///
/// This function does ALL executor mechanics, in one place:
///
///   1. Assembles the `RunConfig` — always
///      [`supervised_with_abort`](RunConfig::supervised_with_abort). The mode
///      is hardcoded because every dynamically spawned child registers via
///      `RegisterDynamicChild`, which always carries an `abort_ref`: a child
///      that can receive `AbortCommand` must run a loop that listens for it,
///      so no per-factory choice exists to expose.
///   2. Spawns `run()` via `R::spawn()` (the Tier 2 spawn capability).
///   3. Derives the cloneable kill handle (the ripcord) from the task handle
///      via `R::kill_handle()`.
///   4. Packs the [`SpawnOutput`] from the remaining parts.
///
/// Handwritten domain factories do not call this directly — system codegen
/// emits the composition `|req, notify| spawn_actor_task(build_parts(req),
/// notify)` as the [`SpawnFn`] passed to [`spawn_dynamic_child`].
///
/// # Type Parameters
///
/// - `R` — the runtime. The `BloxRuntime<Kill = Kill>` bound beyond
///   `SpawnCap` unifies `R::KillHandle` with [`SpawnOutput`]'s
///   `<R::Kill as KillCapability<R>>::Handle` — the platform spawn only
///   exists on runtimes whose kill capability is the `SpawnCap`-backed
///   [`Kill`]; `NoKill` runtimes never spawn dynamically. The bound is free
///   at the concrete runtimes this is ever monomorphized with (Tokio,
///   TestRuntime).
/// - `S` — the child's `MachineSpec`. `S::Ctx: Send` is required because the
///   machine crosses the task boundary inside the spawned future; the
///   `Mailboxes` GAT bound on `MachineSpec` already guarantees
///   `S::Mailboxes<R>: Mailboxes<S::Event>`, so no further bounds are needed.
pub fn spawn_actor_task<R, S>(
    parts: ActorParts<S, R>,
    notify: ActorRef<ChildLifecycleEvent, R>,
) -> SpawnOutput<R>
where
    R: BloxRuntime<Kill = Kill> + SpawnCap,
    S: MachineSpec,
    S::Ctx: Send,
{
    // 1. Run loop configuration — supervised-with-abort is the only mode for
    //    dynamically spawned children (see the doc comment above).
    let config =
        RunConfig::<R>::supervised_with_abort(parts.lifecycle_rx, parts.abort_rx, notify.sender());

    // 2. Spawn the run loop; 3. convert the task handle into the cloneable
    //    ripcord (the task keeps running — drop does not kill).
    let task = R::spawn(run(parts.machine, parts.mailboxes, config, parts.child_id));
    let kill_handle = R::kill_handle(task);

    // 4. Everything the managing blox needs to supervise the child.
    SpawnOutput {
        child_id: parts.child_id,
        lifecycle_ref: parts.lifecycle_ref,
        abort_ref: parts.abort_ref,
        kill_handle,
        policy: parts.policy,
    }
}

/// A blox that manages spawned children implements this to define how
/// `SpawnOutput` is wrapped into its own control-plane message type.
///
/// The associated `RegisterMsg` is the message type the spawn helper sends
/// on the managing blox's control mailbox after a child is spawned.
///
/// The standard child-management control plane implements this via
/// [`ChildCtrlRegistrar`] with `RegisterMsg = ChildCtrl<R>`. A user's custom
/// blox implements it with their own message type.
pub trait ChildRegistrar<R: BloxRuntime> {
    /// The control-plane message type that carries a `SpawnOutput` to the
    /// managing blox. Sent on the managing blox's control mailbox.
    type RegisterMsg: Send + 'static;

    /// Wrap a `SpawnOutput` into the managing blox's registration message.
    fn register(output: SpawnOutput<R>) -> Self::RegisterMsg;
}

/// Spawn a dynamic child actor and register it with the managing blox.
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
pub fn spawn_dynamic_child<R, Req, C>(
    spawn_fn: SpawnFn<R, Req>,
    req: Req,
    control_ref: &ActorRef<C::RegisterMsg, R>,
    notify_ref: &ActorRef<ChildLifecycleEvent, R>,
    from: ActorId,
) -> Result<(), R::TrySendError>
where
    R: BloxRuntime,
    Req: Send + 'static,
    C: ChildRegistrar<R>,
{
    // 1. Call the spawn function — creates channels, constructs child, spawns task
    let output: SpawnOutput<R> = spawn_fn(req, notify_ref.clone());

    // 2. Wrap output into the managing blox's registration message and send it
    let kill_handle = output.kill_handle.clone();
    let msg = C::register(output);
    if let Err(err) = control_ref.try_send(from, msg) {
        // The registration never arrived — leaving the spawned task running
        // would leak a live, unmanaged actor no supervisor can reach. Kill it
        // via the ripcord before returning the error. (No-op on NoKill
        // runtimes — see `KillCapability::CAN_KILL`.)
        R::Kill::kill(kill_handle);
        return Err(err);
    }

    Ok(())
}

/// Registrar for the standard child-management control plane
/// (`ChildCtrl<R>`, from `bloxide-child-management`).
///
/// The wiring layer injects this type when the managing blox consumes the
/// standard control mailbox. It lives here (not in `bloxide-child-management`)
/// because it bridges `SpawnOutput` and `ChildCtrl` — this is the only crate
/// that can name both without a dependency cycle.
pub struct ChildCtrlRegistrar;

impl<R: BloxRuntime> ChildRegistrar<R> for ChildCtrlRegistrar {
    type RegisterMsg = bloxide_child_management::control::ChildCtrl<R>;

    fn register(output: SpawnOutput<R>) -> bloxide_child_management::control::ChildCtrl<R> {
        bloxide_child_management::control::ChildCtrl::RegisterDynamicChild(
            bloxide_child_management::control::RegisterDynamicChild {
                id: output.child_id,
                lifecycle_ref: output.lifecycle_ref,
                abort_ref: output.abort_ref,
                kill_handle: output.kill_handle,
                policy: output.policy,
            },
        )
    }
}
