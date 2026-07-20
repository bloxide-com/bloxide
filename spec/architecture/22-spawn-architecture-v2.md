# Spawn Architecture v2 — Design Spec

> Status: **REVIEWED** — July 2026 (decisions finalized, ready for implementation)
> Supersedes: spec 21 (spawn-architecture), which superseded specs 15–17
> Related: 08 (supervision), 11 (dynamic-actors), 14 (unified-lifecycle), 18 (composable-context-crates), 19 (declarative-wiring)

## 0. Review Decisions (July 2026)

1. **Kill mechanism: dual.** Self-termination (break out of select loop) as
   the fast path + `SpawnCap::kill(handle)` external abort as fallback for stuck
   tasks. The `TaskHandle` is stored in `ChildEntry` (in the supervisor) via
   `R::Kill::Handle` — a type-level enum on `BloxRuntime` that is `()` (ZST) for
   static runtimes (Embassy) and `R::TaskHandle` for dynamic runtimes (Tokio).
   No `Arc<dyn>`, no `alloc` for the static path. See §2.2 and §6.1.

2. **`Option<kill_ref>` in `ChildEntry`:** Fine. The `Option` is on a cheap,
   cloneable `ActorRef`, not on a trait object. Two registration types at the
   API level (`RegisterChild` / `RegisterDynamicChild`) encode the capability
   at the type level; the internal `Option` is just per-child data.

3. **Pool's `dynamic` feature: keep.** The Pool serves as the example of a
   conditionally-dynamic blox. `spawn_fn`, `spawn_ref`, `notify_ref` are gated
   behind `#[cfg(feature = "dynamic")]` on the Pool's crate.

4. **Wiring codegen: in scope.** Implement the `field` selector on
   `source = "actor"` in `system_wiring.rs` as part of this spec. The
   `spawn_fn` reuses the existing `source = "factory"` handler. The
   `source = "supervisor_spawn"` handler and `[supervision.factory]`
   section are removed.

5. **`Lifecycle` variant: auto-generated.** The codegen already auto-generates
   `Lifecycle(LifecycleCommand)` as the first variant in every event enum,
   with `From<LifecycleCommand>`, `LIFECYCLE_TAG`, `LifecycleEvent` impl, and
   helper methods. No special handling needed — the supervisor gets it for
   free when switching to standard codegen.

6. **Action signatures: concrete.** Drop `E: SupervisorEventLike<R>` — action
   functions take `&SupervisorEvent<R>` directly. Drop `{Event}` from TOML
   turbofish. The `SupervisorEventLike` trait is deleted. This matches the
   pattern used by every other blox.

7. **PR strategy: multiple commits, one PR.** The 12-step migration is
   committed incrementally but merged as a single PR.

## 1. Problem

Spec 21 unified spawning under the supervisor but introduced a `SpawnFactory<R>` trait
with an associated `type Request`. That associated type (`F::Request`) infected the
supervisor's event enum, forcing:

- **Paired event enums**: `SupervisorEvent<R>` (static) and `SupervisorEvent<R, F>` (dynamic) — 198 lines of hand-written code in `event.rs`
- **Custom mailboxes**: `dynamic_mailboxes.rs` (103 lines) because `From<Envelope<F::Request>>` hits Rust's coherence checker (E0119) — the compiler can't prove `F::Request` is never `ChildLifecycleEvent`
- **Escape hatches in blox.toml**: `event_name`, `mailboxes_type`, `feature_generics`, `feature_event_generics`, `extra_impls` — the supervisor is the only blox that can't use standard codegen
- **`SupervisorEventLike` trait**: a hand-written abstraction layer so action functions can be generic over both paired event types
- **`#[cfg(feature = "dynamic\")]` everywhere**: on event variants, context fields, transition rules, generics — feature unification bugs across workspace members
- **`KillCap` trait + `Arc<dyn KillCap>`**: a heap-allocated, type-erased registry of task handles — requires `alloc`, breaks `no_std`, and introduces dynamic dispatch in the supervision path

The root cause: **the supervisor was made responsible for spawning.** Spec 21 put a
`Spawn(F::Request)` variant in the supervisor's event enum, which forced `F` onto the
enum, which forced paired enums, custom mailboxes, and escape hatches. The supervisor
became app-specific — it had to know the application's concrete spawn request type.

A secondary root cause: **kill was a function call on a trait object, not a message.**
`KillCap` stored `ActorId → JoinHandle` in a map and called `kill(id)` directly. This
required `Arc<dyn KillCap>` (heap + dynamic dispatch), `Option<Arc<dyn KillCap>>` on
`ChildGroup` (runtime check for a capability that should be a type-level property), and
threading the `KillCap` through the spawn path.

## 2. Key Insight

**Spawning is not a supervisor action. It is a wiring/runtime action that *notifies*
the supervisor.** The supervisor's job is lifecycle management, not child creation.

The supervisor already has a mechanism that decouples "I know a child exists" from
"I know how to create it": **`RegisterChild`**. This message carries only lifecycle
types — `ActorId`, `ActorRef<LifecycleCommand, R>`, `ChildPolicy`. No app-specific
types. The supervisor doesn't care *who* created the child or *how*. It just starts
managing lifecycle.

The existing `spawn_dynamic_supervised_child` function in `bloxide-tokio/src/supervision.rs`
already uses this pattern: it creates the channels, spawns the task, and sends
`RegisterChild` to the supervisor. The supervisor never sees the spawn request type.

**This spec formalizes that pattern.** The spawn decision moves out of the supervisor
and into the requesting blox (e.g., the Pool). The requesting blox owns its concrete
spawn types. The supervisor stays generic — `SupervisorControl<R>` keeps just
`RegisterChild` + `HealthCheckTick`, no `Spawn` variant, no `F`, no app types.

This eliminates `F` from the event enum entirely. The supervisor's event enum becomes
`SupervisorEvent<R>` — one enum, not paired, no `F`. Standard codegen generates
everything. No escape hatches. The supervisor never changes for different apps.

### 2.1 Capabilities as Mailboxes (New Principle)

**Platform capabilities (kill, spawn, suspend, inspect, etc.) are message channels,
not trait objects.** Instead of the supervisor holding a `KillCap` trait object and
calling `kill(id)` as a function, the supervisor holds an `ActorRef<KillCommand, R>`
and sends a `Kill` message. The concrete task handle stays on the receiving side (the
child's task), never crossing the supervisor boundary.

This is the actor model applied to platform capabilities:

- **No `Box<dyn>`, no `Arc<dyn>`** — capabilities are typed mailboxes, not trait objects
- **No `Option<Capability>`** — if a child doesn't support a capability, it doesn't have
  that mailbox. The type system encodes what's available, not a runtime `Option`
- **No concrete-type generics mess** — the supervisor is generic over `R: BloxRuntime`
  only, never `SpawnCap`. The `SpawnCap` trait exists for the runtime, but the supervisor
  never references it
- **Extensible** — future capabilities (suspend, resume, inspect) are just more mailboxes
  with more command enums. No new traits, no new trait bounds on the supervisor
- **Embassy-compatible** — static children (no `SpawnCap`) simply don't have capability
  mailboxes. No `Option`, no `dyn`, no `alloc` needed for the base case

Each dynamically spawned child can have **per-capability mailboxes** in addition to the
base lifecycle mailbox. The supervisor holds the sending ends (`ActorRef`s) and stores
them in its child list. The child's task (or a wrapper around `run_supervised_actor`)
holds the receiving ends. The **concrete `TaskHandle`** is stored in `ChildEntry` in the
supervisor, via the `R::Kill` associated type — see §2.2.

### 2.2 Kill Capability as a Runtime Associated Type

The kill capability is a property of the **runtime**, not the supervisor. Embassy has no
`SpawnCap` and cannot externally abort tasks. Tokio has `SpawnCap` and can. This choice is
encoded at the type level via an associated type on `BloxRuntime`, not via `Arc<dyn>` or
runtime `Option` checks.

```rust
/// Type-level kill capability for a runtime.
///
/// `NoKill` — no external task abort (Embassy, static-only). `Handle = ()` (ZST).
/// `Kill`   — external abort via `SpawnCap::kill(handle)` (Tokio, dynamic).
///
/// This is a type-level enum, not a trait object. The runtime picks the
/// variant; the supervisor is monomorphized for whichever it is.
pub trait KillCapability<R: BloxRuntime> {
    type Handle: Send + 'static;
    fn kill(handle: Self::Handle);
}

pub struct NoKill;
impl<R: BloxRuntime> KillCapability<R> for NoKill {
    type Handle = ();
    fn kill(_: ()) {}
}

pub struct Kill;
impl<R: BloxRuntime + SpawnCap> KillCapability<R> for Kill {
    type Handle = R::TaskHandle;
    fn kill(handle: R::TaskHandle) {
        R::kill(handle);
    }
}
```

On `BloxRuntime`:

```rust
pub trait BloxRuntime: Clone + Send + 'static {
    // ... existing associated types ...
    /// Kill capability. `NoKill` (default) for static runtimes, `Kill` for dynamic.
    type Kill: KillCapability<Self> = NoKill;
    // ... existing methods ...
}
```

Runtime impls:
- **Embassy**: uses default `type Kill = NoKill`. No `SpawnCap` impl needed. `Handle = ()`.
- **Tokio**: `type Kill = Kill`. Requires `TokioRuntime: SpawnCap`. `Handle = JoinHandle<()>`.
- **TestRuntime**: default `NoKill` unless a test specifically exercises kill.

**Key properties:**
- `ChildGroup<R>` stays bounded by `R: BloxRuntime` only — no `SpawnCap` bound leaks.
- The `SpawnCap` bound is satisfied at the runtime impl site, not in the supervisor crate.
- For Embassy: `ChildEntry::task_handle` is `()` (ZST, zero space). No `alloc`.
- For Tokio: `ChildEntry::task_handle` is `JoinHandle<()>`. Stored by value, no `Arc`.
- No `Arc<dyn KillCap>`. No trait object. No dynamic dispatch. No heap allocation.

## 3. Design Principles

1. **No unsupervised children.** Every child — static or dynamic — is registered
   with the supervisor via `RegisterChild`. The supervisor is the sole gateway for
   lifecycle management. No child exists without the supervisor knowing.

2. **Spawning is owned by the requester, not the supervisor.** The blox that wants
   a child (e.g., the Pool) owns the concrete spawn types (`SpawnRequest`,
   `SpawnedWorker`, the spawn function). It calls a runtime spawn helper that creates
   the child, spawns the task, and sends `RegisterChild` to the supervisor. The
   supervisor never sees the spawn request type.

3. **No `Box`, no `dyn`, no dynamic dispatch.** The spawn function is a `fn` pointer
   (concrete, monomorphized). All messages are fully typed. Dispatch is monomorphized
   at compile time. Required for Embassy/microcontroller (no heap).

4. **The supervisor is just another blox.** Standard codegen from `blox.toml`. No
   `event_name`, no `mailboxes_type`, no `extra_impls`, no `feature_generics`. The
   supervisor's `blox.toml` is identical for static and dynamic apps — the `dynamic`
   feature is on the *requesting* blox and the *runtime*, not the supervisor.

5. **Spawning is integrated into the lifecycle system.** The spawn helper creates
   the child, sends `RegisterChild` to the supervisor, and the supervisor sends
   `LifecycleCommand::Start`. The child's lifecycle flows through the existing
   `ChildLifecycleEvent` channel (`Started` → `Alive` → `Done`/`Failed` → `Reset` →
   `Stopped`). No new lifecycle event types needed.

6. **Spawning is async by design.** The requesting blox sends a spawn request (on its
   own channel, not the supervisor's), the spawn helper creates the child and sends
   `RegisterChild` to the supervisor, and the spawn helper sends the app-specific
   handles back to the requester via a reply channel. The child's own initialization
   (which may be slow) happens in the child's task and flows back as `Started` through
   the lifecycle channel.

7. **Everything is codegen-ed.** The `blox.toml` is the source of truth. No hand-written
   event enum, no hand-written mailboxes, no hand-written `MachineSpec` impls. The
   supervisor's `blox.toml` uses only standard codegen features — no escape hatches.

8. **The supervisor is a reference implementation, not a hardcoded singleton.** The
   supervision traits (`ChildGroup`, `ChildPolicy`, `SupervisorControl`) and the
   runtime capabilities (`SpawnCap`, `run_supervised_actor`) are the reusable layer.
   Any blox can include `ChildGroup<R>` in its context and implement supervision. The
   `bloxide-supervisor` blox is the standard reference; other bloxes can compose the
   same traits differently.

9. **Capabilities are mailboxes, not trait objects.** Each platform capability is an
   enum sent on a separate mailbox for that capability. The supervisor sends messages
   on typed channels; the concrete handles stay on the receiving side. No `Arc<dyn>`,
   no `Option<Capability>`, no dynamic dispatch. This is the general model for all
   platform capabilities — kill is the first, but the pattern extends to suspend,
   resume, inspect, etc.

## 4. Architecture

### 4.1 Crate Layout

```
bloxide-core              ← engine + runtime capabilities
  BloxRuntime, MachineSpec, lifecycle types
  DynamicChannelCap (existing)
  SpawnCap (MODIFIED — gains TaskHandle associated type, kill method)
  run_supervised_actor (existing, in each runtime crate)
  KillCap trait ← DELETED (replaced by capability mailboxes)

bloxide-supervisor/       ← the supervisor blox (codegen-ed from blox.toml)
  blox.toml                ← source of truth: states, context, transitions, events
  Cargo.toml               ← NO dynamic feature. Supervisor is feature-free.
  src/generated/           ← codegen output (ctx.rs, topology.rs, spec_skeleton.rs, events.rs)
  src/lib.rs               ← re-exports

bloxide-core/  ← gains child_management module
  child_management: KillCommand, ChildPolicy, GroupShutdown, RestartStrategy
  spawn: SpawnOutput<R>, SpawnFn<R, Req>, ChildRegistrar<R>, spawn_child<R, Req, C>

bloxide-supervisor-context/  ← context crate (hand-written data types + traits)
  ChildGroup<R>             ← MODIFIED: per-child kill_ref + task_handle
  HasChildGroup<R>, HasChildGroupMut<R>, HasPending, HasChildNotify<R> accessor traits
  SupervisorControl<R> (RegisterChild, HealthCheckTick)  ← UNCHANGED, no Spawn variant
  RegisterChild<R>          ← MODIFIED: carries kill_ref for dynamic children
  RegisterDynamicChild<R>   ← NEW: dynamic registration with kill_ref + task_handle
  SupervisorRegistrar       ← NEW: ChildRegistrar impl for standard supervisor

bloxide-supervisor-actions/  ← action crate (hand-written action functions)
  start_children, stop_all_children
  handle_done_or_failed, handle_reset, record_stopped, record_started, record_alive
  register_child, handle_health_check
  (NO handle_spawn_request — spawning is not a supervisor action)
  (kill_child sends a KillCommand message, not a function call)

bloxide-tokio/            ← Tokio runtime (adds spawn helper)
  run_supervised_actor (existing)
  ChildGroupBuilder (existing, extended)
  spawn_supervised_child (NEW — runtime spawn helper, replaces spawn_dynamic_supervised_child)
  run_supervised_actor_with_kill (NEW — wrapper that listens on kill mailbox)

bloxide-embassy/          ← Embassy runtime (unchanged — no dynamic spawning)
  run_supervised_actor (existing)
  ChildGroupBuilder (existing)

pool-messages/            ← Pool's message types (owns concrete spawn types)
  PoolMsg, WorkerMsg, WorkerCtrl, SpawnedWorker (existing)
  SpawnRequest<R>          ← NEW (replaces AppSpawnRequest) — concrete request enum
  SpawnedWorker<R>          ← existing reply type

tokio-pool-demo-impl/     ← Pool's impl crate (owns the spawn function)
  spawn_worker (NEW — fn pointer, replaces AppSpawnFactory struct)

REMOVED:
  KillCap trait             ← replaced by capability mailboxes (KillCommand enum)
  TokioKillCap struct       ← replaced by per-child kill mailbox in run_supervised_actor_with_kill
  SpawnFactory<R> trait    ← replaced by fn pointer in the app's impl crate
  HasSpawnFactory<R> trait ← removed (supervisor doesn't spawn)
  NoSpawnFactory           ← removed (no factory in supervisor)
  NoSpawnRequest           ← removed
  SpawnPolicy              ← removed (use ChildPolicy directly)
  dynamic_mailboxes.rs     ← removed (standard codegen works)
  SupervisorEventLike trait ← removed (single event type, no genericism needed)
  event.rs (hand-written)  ← removed (codegen generates it)
  handle_spawn_request     ← removed (spawning is not a supervisor action)
```

### 4.2 The Spawn Function

The spawn function lives in the **application's impl crate**, not the supervisor's
context crate. It is a `fn` pointer — concrete, monomorphized, no captured state.

```rust
// In the application's impl crate (e.g., tokio-pool-demo-impl)

use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap, SpawnCap},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::{ActorId, ActorRef},
    child_management::{KillCommand, ChildPolicy},
    spawn::SpawnOutput,
};
use pool_messages::{SpawnRequest, SpawnedWorker, WorkerCtrl, WorkerMsg};

/// The concrete spawn function. A stateless `fn` pointer — no captured state.
/// All per-request state comes through SpawnRequest.
///
/// The function:
///   1. Creates channels (R::channel) for the child — lifecycle, domain, control,
///      and a kill capability mailbox
///   2. Constructs the child's context (app-specific)
///   3. Spawns the child task (R::spawn) with `notify` as the report channel,
///      wrapped in `run_supervised_actor_with_kill` so the kill mailbox is listened on
///   4. Sends the app-specific reply via the request's `reply_to` field
///   5. Returns SpawnOutput for supervisor registration (includes kill_ref)
///
/// The function is fast (run-to-completion): channel creation and R::spawn()
/// are non-blocking. The child's own initialization (which may be slow) runs
/// in the child's task and reports back via lifecycle events.
pub fn spawn_worker<R>(req: SpawnRequest<R>, notify: ActorRef<ChildLifecycleEvent, R>) -> SpawnOutput<R>
where
    R: BloxRuntime + SpawnCap + DynamicChannelCap,
{
    match req {
        SpawnRequest::Worker { task_id: _, pool_ref, reply_to } => {
            let worker_id = R::alloc_actor_id();

            // Create channels for the child
            let (ctrl_ref, ctrl_rx) = R::channel::<WorkerCtrl<R>>(worker_id, 16);
            let (domain_ref, domain_rx) = R::channel::<WorkerMsg>(worker_id, 16);
            let (lifecycle_ref, lifecycle_rx) = R::channel::<LifecycleCommand>(worker_id, 4);
            let (kill_ref, kill_rx) = R::channel::<KillCommand>(worker_id, 4);

            // Construct the child's context (app-specific)
            let behavior = WorkerBehavior::<R>::default();
            let worker_ctx = WorkerCtx::new(pool_ref, worker_id, behavior);
            let machine = StateMachine::<WorkerSpec<R, WorkerBehavior<R>>>::new(worker_ctx);

            // Spawn the child task with kill mailbox support (non-blocking).
            // The TaskHandle from R::spawn() is made available to
            // run_supervised_actor_with_kill for the external-abort
            // fallback path (see §4.12). The exact mechanism for threading
            // the handle into the wrapper is an implementation detail
            // (e.g., a shared cell set after spawn returns, or a separate
            // abort task that watches the kill mailbox and calls
            // R::kill(handle) if the main task doesn't self-terminate).
            let notify_sender = notify.sender();
            let task_handle = R::spawn(async move {
                run_supervised_actor_with_kill(
                    machine,
                    (ctrl_rx, domain_rx),
                    lifecycle_rx,
                    kill_rx,
                    worker_id,
                    notify_sender,
                ).await;
            });
            // task_handle is stored for the external-abort fallback (§4.12)

            // Send app-specific handles back to the requester
            let _ = reply_to.try_send(worker_id, SpawnedWorker {
                child_id: worker_id,
                domain_ref: domain_ref.clone(),
                ctrl_ref: ctrl_ref.clone(),
            });

            // Return what the supervisor needs for lifecycle management + kill
            SpawnOutput {
                child_id: worker_id,
                lifecycle_ref,
                kill_ref,       // ← NEW: kill capability mailbox (send side)
                policy: ChildPolicy::Stop,
            }
        }
    }
}
```

Note: no `AppSpawnFactory` struct. No captured state. The function is a `fn`
pointer — `spawn_worker::<R>` is monomorphized at the wiring site. The wiring
layer provides `spawn_worker as SpawnFn<R, SpawnRequest<R>>` to the runtime spawn
helper (§4.11).

The `SpawnFn` type alias is defined in `bloxide-core` (not `bloxide-tokio`) so the
Pool blox can name the type without depending on a specific runtime:

```rust
// In bloxide-core (alongside SpawnCap, DynamicChannelCap)

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
pub type SpawnFn<R, Req> = fn(
    req: Req,
    notify: ActorRef<ChildLifecycleEvent, R>,
) -> SpawnOutput<R>;
```

### 4.3 The Spawn Request

The spawn request lives in the **application's messages crate** (e.g.,
`pool-messages`). It is a concrete enum — no associated types, no generics
beyond `R`. The application defines this enum with variants for each kind of
actor it wants to spawn.

```rust
// In the application's messages crate (e.g., pool-messages)

use bloxide_core::{capability::BloxRuntime, messaging::ActorRef};

/// A spawn request. Concrete enum — no associated types, no generics beyond R.
/// The application defines this enum with variants for each kind of actor
/// it wants to spawn.
///
/// All state needed to construct the child is carried in the request,
/// not captured in a factory struct. This makes the spawn function stateless.
#[derive(Debug, Clone)]
pub enum SpawnRequest<R: BloxRuntime> {
    Worker {
        task_id: u32,
        pool_ref: ActorRef<PoolMsg, R>,       // so the worker can talk to the pool
        reply_to: ActorRef<SpawnedWorker<R>, R>, // where to send the handles back
    },
    // Future variants: JobRunner { ... }, Scheduler { ... }, etc.
}

/// The reply sent back to the requester with the child's handles.
/// The application defines this — it's app-specific.
#[derive(Debug, Clone)]
pub struct SpawnedWorker<R: BloxRuntime> {
    pub child_id: ActorId,
    pub domain_ref: ActorRef<WorkerMsg, R>,
    pub ctrl_ref: ActorRef<WorkerCtrl<R>, R>,
}
```

Key: `SpawnRequest<R>` is in `pool-messages`, not in `bloxide-supervisor-context`.
The supervisor never imports it, never names it, never matches on it. The Pool
does **not** send `SpawnRequest` on any channel — it calls the runtime spawn
helper directly, passing the request by value (§4.10). The supervisor's control
channel only carries `RegisterChild`, which the spawn helper sends after the
child is created.

### 4.4 The Spawn Output

`SpawnOutput<R>` lives in `bloxide-core` because it is NOT supervisor-specific.
Any blox that manages children — our standard supervisor, a user's custom job
dispatcher, a load balancer — needs the same lifecycle refs, kill ref, task
handle, and policy from a spawn operation. The spawn helper is generic over the
registration protocol (§4.4a), so `SpawnOutput` must be in the core layer.

```rust
// In bloxide-core (moved from bloxide-supervisor-context)

use crate::capability::{BloxRuntime, KillCapability};
use crate::lifecycle::LifecycleCommand;
use crate::messaging::{ActorId, ActorRef};
use crate::child_management::{KillCommand, ChildPolicy};

/// What a spawn function returns — the lifecycle and capability refs needed
/// to register the child with whatever blox manages it.
///
/// This type is NOT app-specific and NOT supervisor-specific. It carries only
/// lifecycle types and capability mailbox refs. The app-specific handles
/// (domain_ref, ctrl_ref, etc.) go back to the requester via the spawn
/// request's reply-to channel, not through here.
///
/// The `task_handle` IS here — the spawn function gets it from `R::spawn()` and
/// passes it to the managing blox so it can call `R::Kill::kill(handle)` as the
/// ripcord for unresponsive children. For `NoKill` runtimes this is `()`.
pub struct SpawnOutput<R: BloxRuntime> {
    /// The allocated actor ID for the new child.
    pub child_id: ActorId,
    /// Channel for sending lifecycle commands (Start, Stop, Reset).
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    /// Kill capability mailbox (send side). The managing blox sends KillCommand
    /// here; the child's task receives it and self-terminates (fast path).
    pub kill_ref: ActorRef<KillCommand, R>,
    /// Task handle for external abort (the ripcord). The managing blox calls
    /// `R::Kill::kill(handle)` when the child is unresponsive. `()` for
    /// `NoKill` runtimes, `R::TaskHandle` for `Kill` runtimes.
    pub task_handle: <R::Kill as KillCapability<R>>::Handle,
    /// Supervision policy for this child.
    pub policy: ChildPolicy,
}
```

Note: `policy` is now `ChildPolicy` directly (not `Option<SpawnPolicy>`). The
old `SpawnPolicy` enum and the `to_child_policy()` conversion function are
removed. `SpawnOutput` and `RegisterChild` both use `ChildPolicy`.

### 4.4a ChildRegistrar — Decoupling Spawn from the Supervisor

The spawn helper (§4.11) must work with **any** blox that manages children, not
just our standard supervisor. A user might write a custom job dispatcher, a
load balancer, or their own supervisor variant — each with its own control
protocol and registration message type.

The `ChildRegistrar` trait bridges the generic spawn helper to a blox-specific
registration protocol:

```rust
// In bloxide-core (new)

use crate::capability::BloxRuntime;
use crate::messaging::ActorRef;
use crate::lifecycle::ChildLifecycleEvent;
use crate::spawn::SpawnOutput;

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
    type RegisterMsg: Send + Clone + 'static;

    /// Wrap a `SpawnOutput` into the managing blox's registration message.
    fn register(output: SpawnOutput<R>) -> Self::RegisterMsg;
}
```

The standard supervisor's implementation (in `bloxide-supervisor-context`):

```rust
// In bloxide-supervisor-context

impl<R: BloxRuntime> ChildRegistrar<R> for SupervisorRegistrar {
    type RegisterMsg = SupervisorControl<R>;

    fn register(output: SpawnOutput<R>) -> SupervisorControl<R> {
        SupervisorControl::RegisterDynamicChild(RegisterDynamicChild {
            id: output.child_id,
            lifecycle_ref: output.lifecycle_ref,
            kill_ref: output.kill_ref,
            task_handle: output.task_handle,
            policy: output.policy,
        })
    }
}

/// Marker type for the standard supervisor's registrar implementation.
pub struct SupervisorRegistrar;
```

A user's custom blox would implement their own:

```rust
// In the user's blox crate

impl<R: BloxRuntime> ChildRegistrar<R> for MyJobDispatcherRegistrar {
    type RegisterMsg = MyControlMsg<R>;

    fn register(output: SpawnOutput<R>) -> MyControlMsg<R> {
        MyControlMsg::RegisterWorker {
            id: output.child_id,
            lifecycle: output.lifecycle_ref,
            kill: output.kill_ref,
            task_handle: output.task_handle,
            policy: output.policy,
        }
    }
}
```

The spawn helper (§4.11) is generic over `C: ChildRegistrar<R>`. The wiring
codegen (§4.13) injects the appropriate `ChildRegistrar` type based on which
blox manages the children in the `system.toml`.

### 4.5 The Kill Capability Mailbox

Kill is a **message**, not a function call. The managing blox sends a `KillCommand`
on a per-child kill mailbox. The child's task (wrapped in
`run_supervised_actor_with_kill`) receives it and aborts itself.

```rust
// In bloxide-core (moved from bloxide-supervisor-context, in new child_management module)

/// Command enum for the kill capability mailbox.
///
/// Sent by the managing blox (supervisor or custom) when ChildPolicy::Kill fires.
/// The child's task receives this on its kill mailbox and aborts itself.
///
/// This is the first instance of the capability-as-mailbox pattern.
/// Future capabilities (suspend, resume, inspect) will follow the same
/// pattern: a command enum sent on a per-child mailbox.
#[derive(Debug, Clone)]
pub enum KillCommand {
    /// Kill the child immediately. No callbacks, no graceful shutdown.
    /// The child's task aborts on receipt.
    Kill { child_id: ActorId },
}
```

The kill mailbox is created by the spawn function (§4.2) alongside the lifecycle
and domain channels. The send side (`kill_ref`) goes into `SpawnOutput` →
the managing blox's registration message → the managing blox's child list. The
receive side (`kill_rx`) goes to `run_supervised_actor_with_kill` which listens
on it in the child's task.

### 4.6 The Supervisor Control Enum

**Unchanged from the current code.** No `Spawn` variant is added. This enum
is specific to our standard supervisor — a user's custom child-managing blox
would define its own control enum (see §4.4a `ChildRegistrar`).

```rust
// In bloxide-supervisor-context (unchanged)

/// Supervisor control-plane events delivered through the control mailbox.
#[derive(Debug, Clone)]
pub enum SupervisorControl<R: BloxRuntime> {
    /// Register a child (static or dynamic) with the supervisor.
    /// The child's channels already exist (created by the wiring layer
    /// for static, or by the spawn helper for dynamic). The supervisor
    /// just tracks it for lifecycle management.
    RegisterChild(RegisterChild<R>),

    /// Trigger one health-check round.
    HealthCheckTick,
}
```

The supervisor's control enum is the same for static and dynamic apps. Dynamic
spawning doesn't add a variant — it just means `RegisterChild` arrives at runtime
(from the spawn helper) instead of at startup (from the wiring layer).

### 4.7 RegisterChild

`RegisterChild` now carries a `kill_ref` for dynamically spawned children. For
static children (wired at startup, no `SpawnCap`), the `kill_ref` is not present.

To avoid `Option`, we use two types:

```rust
// In bloxide-supervisor-context

/// Register a static child (wired at startup). No kill capability.
/// Used by the wiring layer for Embassy and static Tokio children.
pub struct RegisterChild<R: BloxRuntime> {
    pub id: ActorId,
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    pub policy: ChildPolicy,
}

/// Register a dynamically spawned child. Has a kill capability mailbox.
/// Used by the spawn helper when SpawnCap is available.
pub struct RegisterDynamicChild<R: BloxRuntime> {
    pub id: ActorId,
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    pub kill_ref: ActorRef<KillCommand, R>,
    /// Task handle for external abort (the ripcord). Stored by value.
    /// `()` for NoKill runtimes, `R::TaskHandle` for Kill runtimes.
    pub task_handle: <R::Kill as KillCapability<R>>::Handle,
    pub policy: ChildPolicy,
}
```

`SupervisorControl<R>` has two registration variants:

```rust
pub enum SupervisorControl<R: BloxRuntime> {
    RegisterChild(RegisterChild<R>),
    RegisterDynamicChild(RegisterDynamicChild<R>),
    HealthCheckTick,
}
```

Both variants are available regardless of runtime — `RegisterDynamicChild` is just
a struct with a `kill_ref` field; it doesn't require `R: SpawnCap` to *name* the
type (the `ActorRef<KillCommand, R>` only needs `R: BloxRuntime`). The supervisor's
`register_child` action handles both variants: adds the child to the list, stores
the `kill_ref` if present, sends `Start`.

This avoids `Option`, avoids `SpawnCap` bounds on the supervisor, and keeps the
type system encoding the capability (static children don't have `kill_ref`).

### 4.8 The Supervisor Event Enum

```rust
// GENERATED by codegen — NOT hand-written

/// The unified event type for supervisor state machines.
/// One enum, no F parameter, no paired variants, no Spawn variant.
#[derive(Debug, Clone)]
pub enum SupervisorEvent<R: BloxRuntime> {
    Child(Envelope<ChildLifecycleEvent>),
    Control(Envelope<SupervisorControl<R>>),
    Lifecycle(LifecycleCommand),
}

// From impls — standard, no coherence problem
impl<R: BloxRuntime> From<Envelope<ChildLifecycleEvent>> for SupervisorEvent<R> { ... }
impl<R: BloxRuntime> From<Envelope<SupervisorControl<R>>> for SupervisorEvent<R> { ... }
impl<R: BloxRuntime> From<LifecycleCommand> for SupervisorEvent<R> { ... }

// EventTag, LifecycleEvent impls — standard codegen
```

Key: `SupervisorEvent<R>` has **no `F` parameter** and **no `Spawn` variant**.
The event enum is identical for static and dynamic — there is no `dynamic`
feature on the supervisor crate at all. The `Spawn` variant that existed in
spec 21's dynamic enum is gone. Spawning is handled outside the supervisor.

Note on `Envelope` wrapping: the generated event enum uses
`Child(Envelope<ChildLifecycleEvent>)` (the standard codegen pattern), while
the current hand-written enum uses `Child(ChildLifecycleEvent)` (unwrapping
the envelope in the `From` impl). The generated form is the standard pattern
used by all other bloxes. Action functions receive `&SupervisorEvent<R>` and
pattern-match through the `Envelope` — this is the same pattern every other
blox uses. The current hand-written `SupervisorEventLike` trait (which extracts
`&ChildLifecycleEvent` and `&SupervisorControl<R>` from the event) is replaced
by direct pattern matching on `SupervisorEvent<R>`.

Note on the `Lifecycle` variant: the codegen **auto-generates** a
`Lifecycle(::bloxide_core::lifecycle::LifecycleCommand)` variant as the first
variant in every event enum (see `events.rs` lines 83-86). It also auto-generates
the `From<LifecycleCommand>` impl, the `LIFECYCLE_TAG` constant, the
`LifecycleEvent` trait impl, and helper methods (`start()`, `reset()`, `stop()`,
`ping()`). This means the supervisor's generated event enum will include the
`Lifecycle` variant automatically — no special handling is needed in the
`blox.toml` or the codegen. The hand-written `SupervisorEvent` had this variant
manually; the generated version gets it for free. This is the standard pattern
for all bloxes (e.g., `WorkerEvent`, `PingEvent`, `PoolEvent` all have it).

**Breaking change to action function signatures:** the current supervisor action
functions are generic over `E: SupervisorEventLike<R>` — a trait abstraction that
exists only because there were paired event enums (`SupervisorEvent<R>` and
`SupervisorEvent<R, F>`). With spec 22 there is a single `SupervisorEvent<R>`, so
the `E` generic and the `SupervisorEventLike` trait are eliminated. Action
functions take `&SupervisorEvent<R>` **concretely** (not generic over `E`) and
pattern-match through `Envelope<ChildLifecycleEvent>` / `Envelope<SupervisorControl<R>>`
directly. This is the same pattern every other blox uses (e.g., `WorkerEvent`
actions pattern-match directly on `&WorkerEvent<R>`).

The TOML transition turbofish changes from `handle_done_or_failed::<{R}, {Ctx}, {Event}>`
to `handle_done_or_failed::<{R}, {Ctx}>` — the `{Event}` substitution is no longer
needed. The codegen's string substitution for `{Event}` in `spec_skeleton.rs`
becomes unused for the supervisor (other bloxes that don't use `{Event}` are
unaffected). All existing action functions (`handle_done_or_failed`, `record_started`,
`record_alive`, `record_stopped`, `handle_reset`, `register_child`,
`handle_health_check`) need their event-extraction logic updated. The migration
path (§10) includes a step for this rewrite.

### 4.9 The Supervisor Context

```rust
// GENERATED by codegen — NOT hand-written

pub struct SupervisorCtx<R: BloxRuntime> {
    pub children: ChildGroup<R>,
    pub self_id: ActorId,
    pub child_notify: ActorRef<ChildLifecycleEvent, R>,
    pub pending: ChildAction,
}
```

No `spawn_fn` field. No `spawn_factory` field. No `F` parameter. No
`#[cfg(feature = "dynamic\")]` on any field. The supervisor context is the same
for static and dynamic apps. The supervisor doesn't spawn — it only registers
and manages lifecycle.

### 4.10 ChildGroup

`ChildGroup` is simplified. It loses the `kill_cap: Option<Arc<dyn KillCap>>`
field and the `kill_child()` method that calls it. Instead, each child entry
carries a `kill_ref` (an `ActorRef<KillCommand, R>`) for the fast-path kill
message, and a `task_handle` (`R::Kill::Handle`) for the external-abort ripcord.
When `ChildPolicy::Kill` fires, the supervisor sends `KillCommand::Kill` on the
`kill_ref` (fast path) **and** calls `R::Kill::kill(handle)` (ripcord).

```rust
// In bloxide-supervisor-context

/// A child entry in the supervisor's child list.
struct ChildEntry<R: BloxRuntime> {
    id: ActorId,
    lifecycle_ref: ActorRef<LifecycleCommand, R>,
    policy: ChildPolicy,
    restarts: usize,
    permanently_done: bool,
    stopped: bool,
    phase: ChildPhase,
    awaiting_alive: bool,
    /// Kill capability mailbox. None for static children (Embassy),
    /// Some for dynamically spawned children (Tokio with SpawnCap).
    kill_ref: Option<ActorRef<KillCommand, R>>,
    /// Task handle for external abort. `()` (ZST) for static runtimes,
    /// `R::TaskHandle` for dynamic runtimes. Stored by value, no Arc.
    /// The supervisor uses this as the ripcord when the child is
    /// unresponsive and the kill mailbox message goes unanswered.
    task_handle: <R::Kill as KillCapability<R>>::Handle,
}
```

Note: `kill_ref` is `Option<ActorRef<KillCommand, R>>` — an `Option` on a *mailbox
ref* (cheap, cloneable, no `dyn`). `task_handle` is `R::Kill::Handle` — a
*concrete type* selected at the type level (`()` for Embassy, `JoinHandle<()>`
for Tokio). Neither is a trait object. The `Option` on `kill_ref` exists because
`ChildGroup` is a single type that handles both static and dynamic children —
the `Option` encodes "this child has a kill mailbox" vs "this child doesn't."

The `handle_done_or_failed` method changes: when `ChildPolicy::Kill` fires, it
sends `KillCommand::Kill { child_id }` on the child's `kill_ref` (fast path)
**and** calls `R::Kill::kill(handle)` (ripcord):

```rust
// In ChildGroup::handle_done_or_failed (simplified)

if policy == ChildPolicy::Kill {
    // Fast path: send kill message. If the child is responsive, it
    // self-terminates via the select loop in run_supervised_actor_with_kill.
    if let Some(kill_ref) = &self.children[idx].kill_ref {
        let _ = kill_ref.try_send(from, KillCommand::Kill { child_id });
    }
    // Ripcord: external abort. Works even if the child is stuck and
    // never polls the kill mailbox. For NoKill runtimes this is a no-op
    // (kill(()) does nothing). For Kill runtimes this calls handle.abort().
    R::Kill::kill(core::mem::take(&mut self.children[idx].task_handle));
    self.children[idx].phase = ChildPhase::Killed;
    self.children[idx].stopped = true;
    self.stopped_count += 1;
    return self.check_shutdown();
}
```

All other `ChildGroup` methods (restart strategy, shutdown logic, phase tracking,
health check) are unchanged. The `ChildGroup` is still a data struct that tracks
children — it just sends a message instead of calling a trait method for kill.

### 4.11 The Runtime Spawn Helper

The spawn helper lives in the **runtime crate** (e.g., `bloxide-tokio`). It is the
bridge between the requesting blox and whatever blox manages children. It calls
the app's spawn function and sends the registration message (typed by
`C: ChildRegistrar<R>`) to the managing blox's control mailbox.

```rust
// In bloxide-tokio (or a generic runtime helper)

use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap, SpawnCap},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::{ActorId, ActorRef},
};
use bloxide_supervisor_context::{
    ChildPolicy, KillCommand, RegisterDynamicChild, SpawnOutput, SupervisorControl,
};

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
///
/// The function:
///   1. Creates channels (R::channel) for the child — including a kill mailbox
///   2. Constructs the child's context (app-specific)
///   3. Spawns the child task (R::spawn) with kill mailbox support
///   4. Sends any typed reply via the request's reply_to field
///   5. Returns SpawnOutput for supervisor registration (includes kill_ref)
///
/// Note: the TaskHandle from R::spawn() stays inside the child's task
/// (in run_supervised_actor_with_kill). The supervisor never sees it.
/// The supervisor only gets the kill_ref (send side of the kill mailbox).

/// Spawn a supervised child actor.
///
/// Called by the requesting blox (e.g., the Pool) — NOT by the supervisor.
/// The requesting blox provides the spawn function and the request.
///
/// This helper:
///   1. Calls the spawn function to create the child (channels, context, task)
///   2. Sends RegisterDynamicChild to the supervisor's control mailbox
///
/// The supervisor receives RegisterDynamicChild and starts managing the child's
/// lifecycle. The supervisor never sees the request type or the TaskHandle.
///
/// # Type Parameters
///
/// - `R` — the runtime (must support SpawnCap + DynamicChannelCap)
/// - `Req` — the application's concrete spawn request type (e.g., SpawnRequest<R>)
///   The runtime helper is generic over `Req` to avoid depending on any app's
///   messages crate.
/// - `C` — the `ChildRegistrar` implementation (see §4.4a). Determines how
///   `SpawnOutput` is wrapped into the managing blox's control-plane message.
///   For our standard supervisor, `C = SupervisorRegistrar`. For a user's
///   custom blox, `C = MyJobDispatcherRegistrar` (or whatever they implement).
///
/// # Parameters
///
/// - `spawn_fn` — the application's concrete spawn function (fn pointer)
/// - `req` — the application's concrete spawn request
/// - `control_ref` — the managing blox's control mailbox (typed by `C::RegisterMsg`)
/// - `notify_ref` — the managing blox's child-notify mailbox (passed to spawn_fn)
/// - `from` — the requester's ActorId (for the registration message sender)
pub fn spawn_child<R, Req, C>(
    spawn_fn: SpawnFn<R, Req>,
    req: Req,
    control_ref: &ActorRef<C::RegisterMsg, R>,
    notify_ref: &ActorRef<ChildLifecycleEvent, R>,
    from: ActorId,
) -> Result<(), R::TrySendError>
where
    R: BloxRuntime + SpawnCap + DynamicChannelCap,
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
```

Note: `SpawnFn<R, Req>` is a type alias for a `fn` pointer with two type
parameters: `R` (the runtime) and `Req` (the application's concrete request
type). The runtime helper is generic over `Req` so it doesn't depend on any
specific app's messages crate, and generic over `C: ChildRegistrar<R>` so it
doesn't depend on any specific managing blox's control protocol. `SpawnFn` and
`ChildRegistrar` are both defined in `bloxide-core` so any blox can name them
without a runtime or supervisor dependency.

The standard supervisor's convenience wrapper:
```rust
// In bloxide-supervisor-context

/// Convenience wrapper that fixes C = SupervisorRegistrar.
pub fn spawn_supervised_child<R, Req>(
    spawn_fn: SpawnFn<R, Req>,
    req: Req,
    control_ref: &ActorRef<SupervisorControl<R>, R>,
    notify_ref: &ActorRef<ChildLifecycleEvent, R>,
    from: ActorId,
) -> Result<(), R::TrySendError>
where
    R: BloxRuntime + SpawnCap + DynamicChannelCap,
    Req: Send + Clone + 'static,
{
    spawn_child::<R, Req, SupervisorRegistrar>(spawn_fn, req, control_ref, notify_ref, from)
}
```

The application's spawn function has the concrete signature:
```rust
fn spawn_worker<R>(req: SpawnRequest<R>, notify: ...) -> SpawnOutput<R>
```
At the wiring site, this is provided as `spawn_worker as SpawnFn<R, SpawnRequest<R>>`.

### 4.12 run_supervised_actor_with_kill

The kill mailbox's receiving end lives in a wrapper around `run_supervised_actor`.
This wrapper listens on the kill mailbox alongside the lifecycle and domain
mailboxes. Kill uses a **dual mechanism**:

1. **Self-termination (fast path):** When the kill mailbox is part of the main
   event `select` loop, the task receives `KillCommand::Kill` and breaks out of
   the run loop. Returning drops the future and aborts the task. This is the
   common case — the kill mailbox is polled alongside lifecycle and domain
   streams, so the task sees the kill message on the next poll.

2. **External abort (fallback):** If the task is stuck (e.g., blocked on a long
   `await` that doesn't yield to the select loop), the `TaskHandle` from
   `R::spawn()` is stored in the wrapper and can be used to call
   `R::kill(handle)` for an immediate external abort. This is the safety net —
   it should rarely fire, but it's available for unresponsive tasks.

```rust
// In bloxide-tokio (or the runtime crate)

use bloxide_core::{
    capability::{BloxRuntime, SpawnCap},
    lifecycle::ChildLifecycleEvent,
    messaging::ActorRef,
    child_management::KillCommand,
};

/// Run a supervised actor with kill mailbox support.
///
/// This wraps `run_supervised_actor` with an additional kill mailbox.
/// When a `KillCommand::Kill` is received, the actor self-terminates
/// immediately (breaks out of the select loop, drops the future).
///
/// Kill uses a dual mechanism:
///   1. Self-termination (fast path): the task breaks out of the select
///      loop and returns. Works when the kill mailbox is actively polled.
///   2. External abort (ripcord): the `TaskHandle` from `R::spawn()` is
///      stored in `ChildEntry` in the supervisor. When the child is
///      unresponsive (not polling the kill mailbox), the supervisor calls
///      `R::Kill::kill(handle)` — e.g., `JoinHandle::abort()` on Tokio.
///
/// This function handles only the self-termination path. The external
/// abort is handled by `ChildGroup::handle_done_or_failed` in the
/// supervisor (see §4.10).
pub async fn run_supervised_actor_with_kill<S: MachineSpec + 'static>(
    machine: StateMachine<S>,
    domain_mailboxes: S::Mailboxes<TokioRuntime>,
    lifecycle_stream: TokioStream<LifecycleCommand>,
    kill_stream: TokioStream<KillCommand>,
    actor_id: ActorId,
    supervisor_notify: TokioSender<ChildLifecycleEvent>,
) {
    // The kill stream is polled alongside the lifecycle and domain streams
    // in a select loop. When KillCommand::Kill is received, break out of
    // the loop and return. This drops the future and ends the task.
    //
    // If the task is stuck (not yielding to the select loop), the
    // supervisor's ripcord (R::Kill::kill) handles it externally.
    // ... (polls lifecycle, domain, and kill streams in a select loop)
}
```

The `TaskHandle` from `R::spawn()` is returned to the spawn helper, which
passes it to the supervisor via `RegisterDynamicChild::task_handle`. The
supervisor stores it in `ChildEntry::task_handle`. In the common case,
self-termination via the select loop is sufficient and the external abort is
never invoked. The external `R::Kill::kill(handle)` is the ripcord for
unresponsive tasks that don't yield to the select loop. This dual approach gives
both clean self-termination and a safety net, without requiring `Arc<dyn KillCap>`
or a task-handle registry in the supervisor.

For static children (wired at startup, no kill mailbox), the existing
`run_supervised_actor` (without kill support) is used unchanged.

### 4.13 The Pool's Spawn Action

The Pool handles spawning in its own state machine. When it receives a
`SpawnWorker` message, it calls the runtime spawn helper directly. The Pool
owns the `spawn_fn` (a `fn` pointer stored in its context) and the `spawn_ref`
(the managing blox's control mailbox ref). The Pool's action code calls
`spawn_child::<R, Req, C>` — the generic spawn helper — or the convenience
wrapper `spawn_supervised_child` if using the standard supervisor.

```rust
// In the Pool's action crate (e.g., pool-actions or pool/src/actions.rs)

use bloxide_core::{capability::BloxRuntime, transition::ActionResult, accessor::HasSelfId};
use pool_messages::{SpawnRequest, SpawnWorker, PoolMsg};

/// Handle a SpawnWorker request: call the spawn helper to create a child,
/// then transition to the Spawning state to wait for the reply.
pub fn handle_spawn_worker<R: BloxRuntime>(
    ctx: &mut PoolCtx<R>,
    ev: &PoolEvent<R>,
) -> ActionResult {
    if let Some(PoolMsg::SpawnWorker(SpawnWorker { task_id })) = ev.msg_payload() {
        ctx.pending_task_id = *task_id;
        ctx.spawn_in_flight = true;

        let req = SpawnRequest::Worker {
            task_id: *task_id,
            pool_ref: ctx.self_ref.clone(),
            reply_to: ctx.spawn_reply_ref.clone(),
        };

        // Call the runtime spawn helper — the Pool owns the spawn_fn
        // and the managing blox's control_ref (wired as spawn_ref).
        // C = SupervisorRegistrar (standard supervisor) — or a user's
        // custom ChildRegistrar impl for a custom managing blox.
        let result = spawn_supervised_child(
            ctx.spawn_fn,           // fn pointer from wiring
            req,
            &ctx.spawn_ref,         // managing blox's control mailbox
            &ctx.notify_ref,        // managing blox's child-notify mailbox
            ctx.self_id(),
        );

        if result.is_err() {
            bloxide_log::blox_log_warn!(
                ctx.self_id(),
                "spawn failed (supervisor control mailbox full), dropping task_id={}",
                task_id
            );
            ctx.spawn_in_flight = false;
        }
    }
    ActionResult::Ok
}
```

The Pool's context gains these fields (replacing the old `spawn_ref` that
pointed to a separate spawn mailbox):

```rust
// In PoolCtx (from pool/blox.toml)

/// The spawn function (fn pointer, provided at wiring time).
/// Only present on runtimes that support dynamic spawning (Tokio).
/// Gated by the Pool's `dynamic` feature.
pub spawn_fn: SpawnFn<R, SpawnRequest<R>>,

/// Ref to the managing blox's control mailbox — used to send the registration
/// message. This replaces the old spawn_ref that pointed to a separate spawn
/// mailbox. The type is determined by the `ChildRegistrar` impl — for the
/// standard supervisor, it's `ActorRef<SupervisorControl<R>, R>`. For a custom
/// managing blox, it's whatever the blox's `ChildRegistrar::RegisterMsg` is.
/// The Pool just holds the ref and passes it to the spawn helper.
pub spawn_ref: ActorRef<SupervisorControl<R>, R>,

/// Ref to the managing blox's child-notify mailbox — passed to the spawn
/// function so the child can report lifecycle events.
pub notify_ref: ActorRef<ChildLifecycleEvent, R>,
```

Note: the Pool's `spawn_ref` now points to the managing blox's **control**
mailbox. For the standard supervisor, the type is `SupervisorControl<R>`. The
Pool imports `SupervisorControl` from `bloxide-supervisor-context` to name the
type — this is acceptable for the standard supervisor. **For a user's custom
managing blox**, the Pool would import the blox's own control message type
instead, and the spawn helper's `C: ChildRegistrar<R>` type parameter resolves
the type. The generic `spawn_child::<R, Req, C>` helper avoids hardcoding the
supervisor dependency — the convenience wrapper `spawn_supervised_child` is
only for the standard supervisor case.

### 4.14 The Wiring

```toml
# In system.toml (tokio-pool-demo)

[system]
runtime = "tokio"
name = "tokio-pool-demo"

[[actors]]
name = "pool"
blox = "pool-blox"

  [actors.inject]
  self_ref = { source = "self" }
  spawn_fn = { source = "factory", crate = "tokio_pool_demo_impl", function = "spawn_worker" }
  spawn_ref = { source = "actor", actor = "supervisor", field = "control" }
  notify_ref = { source = "actor", actor = "supervisor", field = "notify" }
  spawn_reply_ref = { source = "self_secondary", index = 1 }

  # Bootstrap: send 3 SpawnWorker messages to trigger Idle→Active→AllDone
  [[actors.bootstrap]]
  message = "PoolMsg::SpawnWorker"
  payload = { task_id = 0 }

  [[actors.bootstrap]]
  message = "PoolMsg::SpawnWorker"
  payload = { task_id = 1 }

  [[actors.bootstrap]]
  message = "PoolMsg::SpawnWorker"
  payload = { task_id = 2 }

[[supervision]]
supervisor = "bloxide-supervisor"
strategy = "all_for_one"
children = ["pool"]

  [supervision.policies]
  pool = { stop = true }
```

The wiring layer:
1. Provides `spawn_fn` — the `fn` pointer (`spawn_worker` from
   `tokio_pool_demo_impl`). Reuses the existing `source = "factory"` handler.
   The codegen resolves this to `spawn_worker::<TokioRuntime>` at the wiring site.
2. Provides `spawn_ref` — the managing blox's control mailbox ref. Uses
   `source = "actor"` with `field = "control"` — the general injection mechanism
   for any actor's named refs. The Pool uses this to send the registration message
   (via the spawn helper, typed by `C: ChildRegistrar<R>`).
3. Provides `notify_ref` — the managing blox's child-notify mailbox ref. Uses
   `source = "actor"` with `field = "notify"`.
4. Provides `spawn_reply_ref` — the Pool's own secondary mailbox for
   `SpawnedWorker` replies. This is unchanged from the current wiring.

No `supervision.factory` section. No `AppSpawnFactory` struct. The wiring
layer provides the `fn` pointer directly to the Pool's context.

**General injection mechanism — `source = "actor"` with `field` selector.**

The existing `source = "actor"` injects another actor's primary channel ref
(`{actor}_ref`). The new `field` parameter generalizes this to inject any
named ref an actor exposes — not just the primary channel, and not just
channels at all. The `field` defaults to the primary ref (backward compatible).

The codegen maintains a **symbol table** — a registry mapping `(actor_name, field_name)`
to Rust variable idents. Each section registers the symbols it creates:

- Channel section: registers `(actor, "primary")` → `{actor}_ref` for each actor
- Supervisor setup section: registers `(supervisor, "control")` → extracted
  `control_ref` from `ChildGroupBuilder`, `(supervisor, "notify")` → extracted
  `notify_ref`

The injection handler looks up `(actor, field)` in the symbol table:

```rust
// In system_wiring.rs context construction
} else if source.source == "actor" {
    let src_actor = source.actor.as_deref()...;
    let field = source.field.as_deref().unwrap_or("primary");
    let sym = symbol_table.get(src_actor, field)
        .ok_or_else(|| anyhow!("actor '{}' has no ref '{}'", src_actor, field))?;
    let ref_ident = sym.var_ident;
    ctor_args.push(quote! { #ref_ident.clone() });
}
```

This is general because:
- **Any blox can inject any other blox's named refs** — not just channel refs,
  not just supervisor refs. A user's custom job dispatcher blox exposes its own
  `control` and `notify` mailboxes; any spawning blox injects them the same way.
- **Adding a new named ref to any blox** just means registering it in the symbol
  table — no new source type, no codegen change.
- **The `field` parameter is the only schema change** — defaults to `"primary"`,
  fully backward compatible with existing `system.toml` files.
- **Multiple managing bloxs** are handled by the actor name (each has a unique
  name in `system.toml`).

**Supervisor wiring ordering — two-phase split.**

The `ChildGroupBuilder` is used in two phases:

Phase 1 (before context construction): Create builder, extract `control_ref()`
and `notify_ref()`, register them in the symbol table.

Phase 2 (after machine construction): Add children via `spawn_child!`, call
`finish()`, construct `SupervisorCtx`.

```rust
// Generated main (simplified):
#(#channel_stmts)*              // 1. Create channels for all actors
#(#factory_stmts)*              // 2. (none in this example)
// ── Supervisor phase 1: create builder, extract refs ──
#(#supervisor_setup_stmts)*     // 3. Builder + control_ref + notify_ref (registered in symbol table)
// ── Context construction ──
#(#ctx_stmts)*                  // 4. PoolCtx can inject supervisor refs from symbol table
// ── Machine construction ──
#(#machine_stmts)*              // 5. Machines constructed
// ── Supervisor phase 2: add children, finish, construct supervisor ──
#(#supervisor_finish_stmts)*    // 6. spawn_child! + finish() + SupervisorCtx
#(#bootstrap_send_stmts)*       // 7. Bootstrap messages
#(#supervisor_run_stmts)*       // 8. Spawn supervisor + actor tasks
```

The `ChildGroupBuilder` is a single `let mut group` that spans both phases.
Phase 1 extracts refs; phase 2 adds children and consumes the builder.

## 5. The Full Spawn Flow

```
Pool                      Spawn Helper            Managing Blox            Child Task
  |                            |                       |                       |
  | 1. Pool receives           |                       |                       |
  |    SpawnWorker msg         |                       |                       |
  |    (from bootstrap or      |                       |                       |
  |     another actor)         |                       |                       |
  |                            |                       |                       |
  | 2. Pool calls              |                       |                       |
  |    spawn_child             |                       |                       |
  |    (spawn_fn, req,         |                       |                       |
  |     spawn_ref, notify_ref, |                       |                       |
  |     self_id)               |                       |                       |
  |--------------------------->|                       |                       |
  |                            |                       |                       |
  |                            | 3. spawn_fn(req, notify):                     |
  |                            |    create channels    |                       |
  |                            |    (lifecycle, domain,|                       |
  |                            |     ctrl, kill)       |                       |
  |                            |    construct WorkerCtx|                       |
  |                            |    R::spawn(task)     |                       |
  |                            |    (TaskHandle in     |                       |
  |                            |     SpawnOutput)      |                       |
  |                            |---------------------->|                       |
  |                            |                       |                       | 4. Child runs
  |                            |                       |                       |    run_supervised_actor_with_kill
  |                            |                       |                       |    (polls lifecycle,
  |                            |                       |                       |     domain, kill streams)
  |                            |                       |                       |
  |                            | 5. spawn_fn sends     |                       |
  |                            |    SpawnedWorker reply|                       |
  |                            |    via reply_to       |                       |
  |<---------------------------|                       |                       |
  |                            |                       |                       |
  |                            | 6. Send registration  |                       |
  |                            |    msg via            |                       |
  |                            |    C::register(output)|                       |
  |                            |    on control mailbox |                       |
  |                            |---------------------->|                       |
  |                            |                       | 7. register_child:    |
  |                            |                       |    add to ChildGroup  |
  |                            |                       |    store kill_ref +   |
  |                            |                       |     task_handle       |
  |                            |                       |    send Start          |
  |                            |                       |---------------------->|
  |                            |                       |                       |
  |                            |                       |    8. Child reports  |
  |                            |                       |    Started via notify |
  |                            |                       |<----------------------|
  |                            |                       | 9. record_started:   |
  |                            |                       |    mark child Running |
  |                            |                       |                       |
  | 10. Pool has worker refs,  |                       |                       |
  |     sends DoWork to worker |                       |                       |
  |--------------------------------------------------------------------->|
  |                            |                       |                       |
  |                            |                       | 11. Child reports   |
  |                            |                       |    Done via notify    |
  |                            |                       |<----------------------|
  |                            |                           | 12. handle_done:     |
  |                            |                           |     restart/stop     |
  |                            |                           |     per policy        |
  |                            |                           |                       |
  |                            |                           | [if ChildPolicy::Kill]|
  |                            |                           | 13. send KillCommand  |
  |                            |                           |     on kill_ref       |
  |                            |                           |---------------------->|
  |                            |                           |                       | 14. Child task
  |                            |                           |                       |     receives Kill,
  |                            |                           |                       |     drops future,
  |                            |                           |                       |     task aborts
```

Steps 2-6 are fast (run-to-completion in the spawn helper): channel creation +
`R::spawn()` + `RegisterDynamicChild` send are non-blocking. Step 4 (the child's own
initialization) is async — it runs in the child's task and reports back via
`Started` (step 8). The supervisor doesn't wait for the child to initialize;
it registers the child when `RegisterDynamicChild` arrives (step 7) and tracks
lifecycle as events arrive.

The async round-trip (steps 1 → 5) is inherent to the actor model: the Pool
sends a request and waits for the reply. The Pool's `Spawning` state tracks
this wait. The spawn helper is fast — it doesn't block.

Key difference from spec 21: the supervisor is NOT in the spawn path. The
spawn helper calls the spawn function, sends `RegisterDynamicChild` to the supervisor,
and the supervisor registers the child. The supervisor never sees the
`SpawnRequest` type. The supervisor's state machine handles `RegisterDynamicChild`
as a normal control event — the same path used for static children.

Key difference from the original spec 22: kill is a message on a per-child
mailbox (step 13-14), not a function call on a `KillCap` trait object. The
`TaskHandle` never leaves the child's task. The supervisor only has the
`kill_ref` (send side).

## 6. Static vs Dynamic

### Static spawning (Embassy / no_std)

- `bloxide-supervisor` — no `dynamic` feature (there is no `dynamic` feature)
- No `spawn_fn` field in supervisor context (there never was one)
- No `Spawn` variant in `SupervisorControl` (there never was one)
- Children registered via `RegisterChild` (no kill_ref) — channels created at
  wiring time by `ChildGroupBuilder::add_child`
- Supervisor manages lifecycle only — sends Start/Stop/Reset
- `ChildPolicy::Kill` is not available (no kill mailbox). Use `Stop` or `Restart`.
- `R: BloxRuntime` only — no `SpawnCap` needed
- The Pool blox doesn't have `spawn_fn` — it's not wired
- `run_supervised_actor` (without kill support) is used

### Dynamic spawning (Tokio / std)

- `bloxide-supervisor` — same as static (no `dynamic` feature on supervisor)
- `spawn_fn: SpawnFn<R, SpawnRequest<R>>` field in **the Pool's** context (not the supervisor's)
- `spawn_ref` points to supervisor's control mailbox (for `RegisterDynamicChild`)
- The Pool calls `spawn_supervised_child` (runtime helper) directly
- `R: BloxRuntime + SpawnCap` — runtime supports task spawning
- Application provides `spawn_fn` at wiring time
- The spawn helper creates children (including kill mailbox) and sends
  `RegisterDynamicChild` to the supervisor
- Supervisor registers and manages lifecycle — same code path as static,
  plus stores `kill_ref` and `task_handle` for `ChildPolicy::Kill`
- `ChildPolicy::Kill` sends `KillCommand::Kill` on `kill_ref` (fast path)
  and calls `R::Kill::kill(handle)` (ripcord)
- `run_supervised_actor_with_kill` (with kill mailbox support) is used

The `dynamic` feature (if any) is on the **Pool's** crate, not the supervisor's.
The Pool gates its `spawn_fn` field, `spawn_ref` field, and spawn-related
transitions behind `#[cfg(feature = "dynamic")]`. The supervisor has no
`dynamic` feature at all.

### 6.1 SpawnCap Trait (Modified)

`SpawnCap` gains an associated `TaskHandle` type and a `kill` method. The
`KillCap` trait is deleted entirely.

```rust
// In bloxide-core

/// Tier 2 capability for runtimes that support spawning and killing actor tasks.
///
/// Extends `DynamicChannelCap` (which provides `alloc_actor_id` and `channel`).
/// Blox crates that need dynamic spawning declare `R: SpawnCap`.
/// Embassy does NOT implement this trait — use static wiring for Embassy.
///
/// The `TaskHandle` is a concrete type (not `dyn`) stored by value.
/// For Tokio, this is `JoinHandle<()>`. For future Embassy task pools,
/// it would be whatever Embassy's pool gives back.
pub trait SpawnCap: DynamicChannelCap {
    /// Concrete task handle. Stored by value, never in `Arc<dyn>`.
    type TaskHandle: Send + 'static;

    /// Spawn a future as an independent task. Returns the handle.
    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self::TaskHandle;

    /// Abort a task by handle. Consumes the handle.
    /// For Tokio, this calls `JoinHandle::abort()`.
    fn kill(handle: Self::TaskHandle);
}
```

Note: in the capability-as-mailbox pattern, kill uses a dual mechanism.
The **fast path** is self-termination: the child's task receives
`KillCommand::Kill` on its kill mailbox (polled in the select loop) and
breaks out of the run loop, dropping the future. No external call is needed.
The **ripcord** is external abort: if the task is stuck (not yielding to
the select loop), `R::Kill::kill(handle)` is called with the `TaskHandle`
stored in `ChildEntry::task_handle` in the supervisor. The supervisor sends
the message (fast path) and holds the handle (ripcord). For `NoKill` runtimes
(Embassy), the ripcord is a no-op — `kill(())` does nothing.

### 6.2 KillCapability Trait (New) and KillCap Trait (Deleted)

The old `KillCap` trait is **deleted entirely**. It is replaced by the
`KillCapability<R>` trait and the `NoKill` / `Kill` type-level enum (see §2.2).
The following are removed:

- `KillCap` trait in `bloxide-core`
- `TokioKillCap` struct in `bloxide-tokio`
- `Arc<dyn KillCap>` in `ChildGroup`
- `Option<Arc<dyn KillCap>>` field in `ChildGroup`
- `with_kill_cap()` and `set_kill_cap()` methods on `ChildGroup`
- `kill_cap` field in `ChildGroupBuilder`
- All `KillCap` imports and trait bounds

## 7. What This Eliminates

| Removed | Lines | Why |
|---|---|---|
| `KillCap` trait + `TokioKillCap` struct | ~60 | Replaced by `KillCommand` message on per-child mailbox |
| `Arc<dyn KillCap>` in `ChildGroup` | — | No dynamic dispatch, no heap allocation for kill |
| `SpawnFactory<R>` trait + associated `type Request` | ~30 | Replaced by `fn` pointer in app's impl crate |
| `F` generic on `SupervisorEvent` | — | `Spawn` variant removed from supervisor entirely |
| `F` generic on `SupervisorCtx` | — | Supervisor doesn't spawn — no `spawn_fn` field |
| Paired `SupervisorEvent<R>` / `SupervisorEvent<R, F>` | 198 | One enum, no `F`, no `Spawn` variant |
| `dynamic_mailboxes.rs` | 103 | `From<Envelope<SpawnRequest<R>>>` not needed — supervisor never sees `SpawnRequest` |
| `SupervisorEventLike` trait | 30 | Single event type — action functions take `&SupervisorEvent<R>` |
| `HasSpawnFactory<R>` trait | 10 | Removed — supervisor doesn't spawn |
| `NoSpawnFactory` / `NoSpawnRequest` | 20 | Removed — no factory in supervisor |
| `AppSpawnFactory` struct | 25 | Replaced by `fn spawn_worker<R>(...)` |
| `SpawnPolicy` enum + `to_child_policy()` | 15 | Use `ChildPolicy` directly in `SpawnOutput` |
| `handle_spawn_request` action | 45 | Spawning is not a supervisor action |
| `Spawn` variant in `SupervisorEvent` | — | Spawning is not a supervisor event |
| `Spawn` mailbox in supervisor | — | Pool sends on supervisor's control mailbox, not a separate spawn mailbox |
| blox.toml escape hatches | — | `event_name`, `mailboxes_type`, `feature_generics`, `feature_event_generics`, `feature_mailboxes_type` all removed |
| `dynamic` feature on supervisor | — | Supervisor is feature-free — no dynamic-specific code |
| **Total hand-written code removed** | **~535 lines** | |

## 8. What Stays

- `SpawnCap` in `bloxide-core` — runtime capability for task spawning + killing
  (MODIFIED: gains `TaskHandle` associated type, `kill` method)
- `DynamicChannelCap` in `bloxide-core` — runtime channel creation (unchanged)
- `ChildGroup<R>` in `bloxide-supervisor-context` — pure data, lifecycle tracking
  (MODIFIED: loses `kill_cap` field, gains per-child `kill_ref` + `task_handle`)
- `ChildPolicy`, `RestartStrategy`, `GroupShutdown` — policy enums (MOVED to
  `bloxide-core` child_management module; `Kill` is now implemented via message
  instead of trait call)
- `KillCommand` in `bloxide-core` child_management module — NEW: message type
  for the kill capability mailbox (moved from bloxide-supervisor-context)
- `SpawnOutput<R>` in `bloxide-core` — MOVED from bloxide-supervisor-context
  (generic, not supervisor-specific)
- `ChildRegistrar<R>` in `bloxide-core` — NEW: trait bridging generic spawn
  helper to blox-specific registration protocol
- `RegisterChild` in `SupervisorControl` — static child registration (unchanged)
- `RegisterDynamicChild` in `SupervisorControl` — NEW: dynamic child
  registration with kill_ref
- `KillCommand` enum — NEW: message type for the kill capability mailbox
- `SupervisorControl<R>` — control enum (MODIFIED: adds `RegisterDynamicChild`
  variant, no `Spawn` variant)
- `ChildLifecycleEvent` — lifecycle event types (unchanged)
- All supervisor lifecycle action functions — `start_children`, `stop_all_children`,
  `handle_done_or_failed`, `handle_reset`, `record_started`, `record_alive`,
  `record_stopped`, `register_child`, `handle_health_check` (MODIFIED: event
  extraction changes from `SupervisorEventLike` to direct `Envelope` pattern
  matching; `handle_done_or_failed` sends `KillCommand` instead of calling `kill_cap`)
- `run_supervised_actor` in each runtime crate — child run loop (unchanged for static)
- `run_supervised_actor_with_kill` in `bloxide-tokio` — NEW: run loop with kill
  mailbox support for dynamic children
- `ChildGroupBuilder` in each runtime crate — wiring-time child setup (MODIFIED:
  loses `kill_cap` field; already exposes `control_ref()` and `notify_ref()`)
- `bloxide-peers` crate — peer introduction (unchanged)
- `extra_impls` in blox.toml — general codegen feature, stays (not a
  supervisor-specific escape hatch)

## 9. Why `fn` Pointer Instead of Trait

A `fn` pointer is the simplest type that works:

1. **Concrete type.** `SpawnFn<R, Req>` is `fn(Req, ...) -> SpawnOutput<R>`.
   No associated types, no trait bounds, no generics beyond `R` and `Req`. The event enum
   doesn't carry `SpawnRequest` at all (it's in the Pool's mailbox, not the
   supervisor's). No coherence problem.

2. **No captured state.** All per-request state goes through `SpawnRequest`. The
   Pool passes `pool_ref` (its `self_ref`) in the request. The spawn function is
   stateless. No factory struct to store, no lifetime issues, no cloning.

3. **Monomorphized.** The `fn` pointer is resolved at compile time. The wiring
   layer provides the concrete function. No `Box<dyn>`, no dynamic dispatch.

4. **KillCap is not needed in the spawn function.** The kill mailbox is created
   inside the spawn function (alongside the lifecycle channel). The `TaskHandle`
   from `R::spawn()` stays in the child's task. The supervisor only gets the
   `kill_ref` (send side). No `KillCap` threading, no `&'static` hack, no `static`
   singleton.

5. **If state is needed in the future:** the state can go in the `SpawnRequest`
   (per-request), in the Pool's context (shared, accessible via accessor
   traits), or in a `&'static` (compile-time constant). For the common case,
   `fn` pointer + request data is sufficient. If a future use case genuinely
   needs captured state that can't go in the request, the `fn` pointer can be
   replaced with a small concrete struct that implements `Fn` — but that's a
   future decision, not needed now.

## 10. Migration Path

### Step 1: Modify SpawnCap, delete KillCap

- Add `type TaskHandle: Send + 'static` associated type to `SpawnCap`
- Add `fn kill(handle: Self::TaskHandle)` to `SpawnCap`
- Change `SpawnCap::spawn` to return `Self::TaskHandle` (was `()`)
- Implement for `TokioRuntime`: `TaskHandle = tokio::task::JoinHandle<()>`,
  `spawn` returns `tokio::spawn(future)`, `kill` calls `handle.abort()`
- Delete `KillCap` trait from `bloxide-core`
- Delete `TokioKillCap` struct from `bloxide-tokio`
- Remove `Arc<dyn KillCap>` from `ChildGroup` (remove `kill_cap` field,
  `with_kill_cap()`, `set_kill_cap()` methods)
- Remove `kill_cap` from `ChildGroupBuilder`

### Step 2: Add child_management module to bloxide-core and capability mailbox types

- Add `child_management` module to `bloxide-core`: `KillCommand`, `ChildPolicy`,
  `GroupShutdown`, `RestartStrategy` (moved from `bloxide-supervisor-context`)
- Add `spawn` module to `bloxide-core`: `SpawnOutput<R>` (moved),
  `ChildRegistrar<R>` trait (new), `spawn_child<R, Req, C>` generic helper (new)
- Add `RegisterDynamicChild<R>` struct to `bloxide-supervisor-context` (with
  `kill_ref` + `task_handle` fields)
- Add `RegisterDynamicChild` variant to `SupervisorControl<R>`
- Add `SupervisorRegistrar` + `ChildRegistrar` impl to `bloxide-supervisor-context`
- Modify `SpawnOutput<R>`: add `kill_ref` + `task_handle` fields, change `policy`
  to `ChildPolicy` (not `Option<SpawnPolicy>`)
- Remove `SpawnPolicy` enum and `to_child_policy()` converter

### Step 3: Simplify bloxide-supervisor-context

- Remove `SpawnFactory<R>` trait, `HasSpawnFactory<R>`, `NoSpawnFactory`, `NoSpawnRequest`
- Remove `KillCommand`, `ChildPolicy`, `GroupShutdown`, `RestartStrategy`,
  `SpawnOutput` from `bloxide-supervisor-context` (moved to `bloxide-core`)
- Add `SupervisorRegistrar` marker type + `ChildRegistrar<R>` impl
- Keep `SupervisorControl<R>`: `RegisterChild` + `RegisterDynamicChild` +
  `HealthCheckTick` (no `Spawn` variant)
- Keep `HasChildNotify<R>` (the managing blox still needs the notify ref)
- Keep `ChildGroup<R>` (modified — stays here, it's the standard supervisor's
  data structure; a custom managing blox would have its own)

### Step 4: Move SpawnRequest to pool-messages

- Rename `AppSpawnRequest<R>` → `SpawnRequest<R>` (or keep the name — it's app-specific)
- Add `pool_ref` field to `SpawnRequest::Worker` (currently captured in `AppSpawnFactory`)
- `SpawnRequest::Worker { task_id, pool_ref, reply_to }` — all state in the request

### Step 5: Rewrite spawn function

- Replace `AppSpawnFactory` struct + `SpawnFactory` impl with `fn spawn_worker<R>(...)`
- The function takes `SpawnRequest<R>` + `notify`, returns `SpawnOutput<R>`
- All state comes from the request, no captured state
- The function creates a kill mailbox channel alongside the lifecycle channel
- The function calls `R::spawn()` and wraps in `run_supervised_actor_with_kill`
- The function returns `kill_ref` in `SpawnOutput`

### Step 6: Add runtime spawn helper + kill-aware run loop

- Add `spawn_supervised_child` to `bloxide-tokio` (replaces `spawn_dynamic_supervised_child`)
- The helper takes `spawn_fn`, `req`, `control_ref`, `notify_ref`, `from`
- It calls the spawn function, sends `RegisterDynamicChild` to the supervisor
- Remove the old `spawn_dynamic_supervised_child`
- Add `run_supervised_actor_with_kill` to `bloxide-tokio` — wraps
  `run_supervised_actor` with kill mailbox polling
- Add `SpawnFn<R, Req>` type alias to `bloxide-core`

### Step 7: Update supervisor blox.toml

- Remove all escape hatches: `event_name`, `mailboxes_type`, `feature_generics`,
  `feature_event_generics`, `feature_mailboxes_type`, `feature_imports`,
  `feature_spec_imports`
- Add standard `[event]` section with `[[event.mailboxes]]` for Child and Control
- Remove `spawn_factory` context field (no factory in supervisor)
- Remove `spawn_fn` context field (no spawn function in supervisor)
- Remove the spawn transition (`SupervisorEvent::Spawn(_)`)
- Remove `feature = "dynamic"` from all fields and transitions
- Remove `dynamic` feature from `Cargo.toml`
- Keep `extra_impls` for `HasPending` and `all_children_stopped()`

### Step 8: Update Pool blox.toml

- Add `spawn_fn` field (type `SpawnFn<R, SpawnRequest<R>>`, role `ctor`) — gated
  by Pool's `dynamic` feature
- Change `spawn_ref` type from `ActorRef<AppSpawnRequest<R>, R>` to
  `ActorRef<SupervisorControl<R>, R>` — add import of `SupervisorControl` from
  `bloxide-supervisor-context` (standard supervisor case). For a custom
  managing blox, import the blox's own control message type instead.
- Add `notify_ref` field (type `ActorRef<ChildLifecycleEvent, R>`, role `ctor`)
- Update `handle_spawn_worker` to call `spawn_child` (generic) or
  `spawn_supervised_child` (standard supervisor convenience wrapper) instead
  of sending to a separate spawn mailbox
- Gate `spawn_fn`, `spawn_ref`, `notify_ref`, and spawn-related transitions behind
  the Pool's `dynamic` feature (the Pool's `Cargo.toml` defines `dynamic`, not
  the supervisor's)

### Step 9: Rewrite supervisor action functions for concrete event type

- **Drop the `E: SupervisorEventLike<R>` generic** from all action functions.
  There is one event type (`SupervisorEvent<R>`), not paired enums. Action
  functions take `ev: &SupervisorEvent<R>` concretely — same pattern as every
  other blox.
- Replace `ev.as_child_event()` with direct pattern matching:
  `if let SupervisorEvent::Child(Envelope(_, child_ev)) = ev { ... }`
- Replace `ev.as_control_event()` with direct pattern matching:
  `if let SupervisorEvent::Control(Envelope(_, control_ev)) = ev { ... }`
- Update `handle_done_or_failed`: when `ChildPolicy::Kill`, send `KillCommand::Kill`
  on the child's `kill_ref` (if present) instead of calling `kill_cap.kill(id)`
- Update `register_child`: handle both `RegisterChild` (static) and
  `RegisterDynamicChild` (dynamic) — store `kill_ref` if present
- Update TOML transitions: drop `{Event}` from turbofish —
  `handle_done_or_failed::<{R}, {Ctx}>` instead of `handle_done_or_failed::<{R}, {Ctx}, {Event}>`
- Delete `SupervisorEventLike` trait and all impls from `bloxide-supervisor-context`
- All other action functions: update event extraction from `SupervisorEventLike`
  to `Envelope` pattern matching

### Step 10: Delete hand-written supervisor files

- Delete `crates/bloxide-supervisor-context/src/event.rs` (198 lines) — codegen generates it
- Delete `crates/bloxide-supervisor/src/dynamic_mailboxes.rs` (103 lines) — standard codegen
- Remove `SupervisorEventLike` trait and all impls
- Remove `HasSpawnFactory` trait and impls
- Remove `handle_spawn_request` from `bloxide-supervisor-actions/src/spawn.rs`
  (or delete the entire file — the supervisor has no spawn actions)

### Step 11: Regenerate and verify

- `cargo install --path crates/tools/cargo-blox --force`
- `cargo blox generate --workspace .`
- `cargo test --workspace`
- Verify: supervisor's generated `events.rs` uses standard `Envelope<M>` pattern
- Verify: no `F` parameter anywhere in generated supervisor code
- Verify: no `Spawn` variant in `SupervisorEvent` or `SupervisorControl`
- Verify: `From<Envelope<SupervisorControl<R>>>` compiles (no E0119)
- Verify: supervisor's `blox.toml` has no escape hatches
- Verify: `KillCap` trait is deleted, no `Arc<dyn KillCap>` anywhere
- Verify: `ChildGroup` has no `kill_cap` field, uses `kill_ref` per child
- Verify: all existing tests pass

### Step 12: Update wiring

- Update `system.toml`: remove `[supervision.factory]` section
- Add `spawn_fn` injection: `{ source = "factory", crate = "tokio_pool_demo_impl", function = "spawn_worker" }`
- Change `spawn_ref` source to: `{ source = "actor", actor = "supervisor", field = "control" }`
- Add `notify_ref` injection: `{ source = "actor", actor = "supervisor", field = "notify" }`
- Add `field` selector to `InjectSource` schema in `schema.rs` (defaults to `"primary"`)
- Implement symbol table in `system_wiring.rs` and split supervisor wiring into
  two phases (setup before ctx, finish after machines)
- Remove old `source = "supervisor_spawn"` handler and `[supervision.factory]` codegen
- Verify pool demo works end-to-end

## 11. Resolved Questions

1. **`fn` pointer vs closure.** A `fn` pointer can't capture state. If a future
   spawn function needs captured state (counter, rate limiter, connection pool),
   the state must go in the `SpawnRequest` or the Pool's context. This is the
   chosen design (§9): `fn` pointer is the type, with all per-request state in
   `SpawnRequest`. If a future use case genuinely needs captured state that
   can't go in the request, the `fn` pointer can be replaced with a small
   concrete struct that implements `Fn` — but that's a future decision, not
   needed now.

2. **`extra_impls` in blox.toml.** `HasPending` and `all_children_stopped()` are
   still in `extra_impls`. These could move to `#[provides]` / `#[provides_mut]`
   on context fields, or stay as `extra_impls` (they're small). Not blocking —
   stays as-is for this spec.

3. **Root transitions.** `MachineSpec::root_transitions()` still defaults to `&[]`
   as a hand-written trait method. Adding `[[topology.root_transitions]]` to the
   schema + codegen (~80 lines) is a separate cleanup, not part of this spec.

4. **Generic spawn helper.** The `spawn_child<R, Req, C>` helper is generic
   over `R: SpawnCap + DynamicChannelCap` and `C: ChildRegistrar<R>`. The
   Tokio runtime provides it; Embassy doesn't need it (no dynamic spawning).
   The convenience wrapper `spawn_supervised_child` fixes `C = SupervisorRegistrar`
   for the standard supervisor case.

5. **Pool's `spawn_ref` type change.** The Pool's `spawn_ref` changes from
   `ActorRef<AppSpawnRequest<R>, R>` to `ActorRef<SupervisorControl<R>, R>`
   (for the standard supervisor). For a custom managing blox, the type is
   whatever the blox's `ChildRegistrar::RegisterMsg` resolves to. The
   generic `spawn_child` helper avoids hardcoding the supervisor dependency.

6. **Future capabilities.** The capability-as-mailbox pattern (§2.1) is
   generalizable to suspend, resume, inspect, etc. Each would be a command enum
   sent on a per-child mailbox. The child's task would listen on it alongside
   the kill and lifecycle mailboxes. This is a future design direction, not
   part of this spec — but the spec is designed to not prevent it.

7. **Embassy task pools.** Embassy may eventually support dynamic spawning via
   task pools (multiple instances of a task type). The `SpawnCap` trait is
   designed to accommodate this: `TaskHandle` would be whatever Embassy's pool
   gives back. The capability-as-mailbox pattern works: the kill mailbox is
   just another channel. This is a future direction, not part of this spec —
   but the spec is designed to not prevent it.

8. **Kill mechanism threading (RESOLVED).** The `TaskHandle` from `R::spawn()`
   is stored in `ChildEntry::task_handle` in the managing blox, via the
   `R::Kill` associated type on `BloxRuntime` (see §2.2). No shared cell,
   no separate abort task, no `Arc<Mutex<Option<...>>>`. The handle is a
   concrete type (`()` for `NoKill`, `R::TaskHandle` for `Kill`) stored by
   value. The managing blox calls `R::Kill::kill(handle)` as the ripcord when
   `ChildPolicy::Kill` fires. This is no longer an implementation detail —
   the threading mechanism is fully specified by the `KillCapability` trait.

9. **General spawn capability (RESOLVED).** The spawn mechanism is decoupled
   from the standard supervisor via `ChildRegistrar<R>` (§4.4a). Any blox that
   manages children implements `ChildRegistrar` with its own `RegisterMsg` type.
   The generic `spawn_child<R, Req, C>` helper works with any managing blox.
   The wiring codegen uses `source = "actor"` with `field` selector — no
   supervisor-specific source types. The standard supervisor is just one
   possible child manager; users can write their own.

## 12. Relationship to Lifecycle

Spawning is integrated into the lifecycle system through the control channel:

- **Birth**: The spawn helper creates the child and sends the registration
  message (via `C::register(output)`) to the managing blox's control mailbox.
  For the standard supervisor: `SupervisorControl::RegisterDynamicChild(RegisterDynamicChild { id, lifecycle_ref, kill_ref, task_handle, policy })`.
  The managing blox registers it in its child list.
- **Start**: The managing blox sends `LifecycleCommand::Start` to the child
  (in the `register_child` action, immediately after adding to the child list).
  The child reports `Started` via the notify channel.
- **Running**: The child reports `ChildLifecycleEvent::Started` → `Alive`.
- **Completion**: The child reports `Done` or `Failed`.
- **Restart**: The supervisor sends `Reset`, child reports `Reset` → `Started`.
- **Shutdown**: The supervisor sends `Stop`, child reports `Stopped`.
- **Kill**: The supervisor sends `KillCommand::Kill` on the child's `kill_ref`
  (fast path — child self-terminates via select loop) and calls
  `R::Kill::kill(task_handle)` (ripcord — external abort for stuck tasks).
  No `Arc<dyn>`, no trait object. For `NoKill` runtimes the ripcord is a no-op.

The `RegisterChild` message is the entry point for static children. The
`RegisterDynamicChild` message is the entry point for dynamic children. Both
go to the supervisor's control mailbox, and the supervisor's `register_child`
action handles both: add to `ChildGroup`, send `Start`. The only difference is
`RegisterDynamicChild` carries a `kill_ref` that the supervisor stores for
`ChildPolicy::Kill`.

The child's `run_supervised_actor` loop (or `run_supervised_actor_with_kill`
for dynamic children) handles lifecycle reporting automatically — it converts
`DispatchOutcome` to `ChildLifecycleEvent` and sends it to the supervisor's
`child_notify` mailbox. This is unchanged.

The key difference from spec 21: `RegisterChild`/`RegisterDynamicChild` is the
*only* message the supervisor receives about a new child. There is no separate
`Spawn` event. The spawn helper (outside the supervisor) creates the child and
sends `RegisterDynamicChild`. The supervisor's state machine handles it as a
normal control event — the same code path used for static children wired at
startup, plus storing the `kill_ref`.
