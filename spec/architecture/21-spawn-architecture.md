# Spawn Architecture — Design Spec

> Status: **DRAFT** — July 2026
> Supersedes: specs 15 (supervisor-owned-spawning), 16 (spawn-service), 17 (spawn-cap-design)
> Related: 08 (supervision), 11 (dynamic-actors), 18 (composable-context-crates)

## 1. Problem

The current spawning architecture has three parallel, disconnected systems:

1. **Factory injection** (pool demo) — the Pool holds a `WorkerSpawnFn<R>` closure, calls it directly, gets refs back. No supervisor involvement.
2. **`bloxide-spawn` crate** — `SpawnCap`, `SpawnFactoryFor`, `ErasedSpawnFactory`, `PeerCtrl`, `introduce_peers`. A separate universe of spawn abstractions not connected to the supervisor.
3. **`bloxide-supervisor` crate** — `ChildGroup`, `RegisterChild`, lifecycle management. Can track children but doesn't create them.

The result: dynamic children are **unsupervised**. The supervisor only knows about children that someone else created and registered. Specs 15–17 tried to bridge this gap but went in circles on type erasure and `no_alloc` constraints.

## 2. Design Principles

1. **No unsupervised children.** The supervisor is the sole gateway for child actor creation. All actors — static and dynamic — are registered with and managed by the supervisor.

2. **No `Box`, no `dyn`, no dynamic dispatch.** All factories are concrete structs stored by value. All control messages are fully typed. Dispatch is monomorphized at compile time. Required for Embassy/microcontroller (no heap).

3. **The supervisor is just another blox.** It has a `blox.toml`, is fully codegen-ed, and its behavior lives in context and action crates. No hand-written `MachineSpec` impls or handler tables outside of action crates.

4. **Spawning is a platform service.** `SpawnCap` is a runtime capability (like `DynamicChannelCap`). The factory calls `R::spawn()` internally. The supervisor never calls `R::spawn()` directly — it calls the factory, which is a concrete struct provided by the application.

5. **Static vs dynamic is a feature gate.** One supervisor crate, one `blox.toml`. The `dynamic` feature adds the spawn factory field, the `Spawn` event variant, and the spawn transition rules. Without the feature, the supervisor manages static children only and needs only `R: BloxRuntime`.

6. **Everything is codegen-ed.** The `blox.toml` is the source of truth. The codegen produces the context struct, event enum, topology, and `MachineSpec` impl. The only hand-written code is action function bodies (in action crates) and data structures/traits (in context crates).

## 3. Architecture

### 3.1 Crate Layout

```
bloxide-core              ← engine + runtime capabilities (unchanged + SpawnCap added)
  BloxRuntime, MachineSpec, lifecycle types
  DynamicChannelCap, KillCap (existing)
  SpawnCap (NEW — moved from bloxide-spawn: fn spawn(future))

bloxide-supervisor/       ← the supervisor blox (codegen-ed from blox.toml)
  blox.toml                ← source of truth: states, context, transitions, events
  Cargo.toml               ← features: default = [], dynamic = ["bloxide-supervisor-context/spawn"]
  src/generated/           ← codegen output (ctx.rs, topology.rs, spec.rs, event.rs)
  src/lib.rs               ← re-exports

bloxide-supervisor-context/  ← context crate (hand-written data types + traits)
  ChildGroup<R>, ChildPolicy, GroupShutdown, RestartStrategy, ChildAction
  HasChildGroup<R> accessor trait
  SupervisorControl<R> (RegisterChild, HealthCheckTick)
  SpawnFactory<R> trait (associated type Request, fn spawn())  ← folded from bloxide-spawn
  SpawnOutput<R>, SpawnPolicy                                ← folded from bloxide-spawn

bloxide-supervisor-actions/  ← action crate (hand-written action functions)
  start_children, stop_all_children
  handle_done_or_failed, handle_reset, record_stopped, record_started, record_alive
  register_child, handle_health_check
  handle_spawn_request (#[cfg(feature = "dynamic")])

bloxide-peers/            ← peer introduction (extracted from old bloxide-spawn)
  PeerCtrl, introduce_peers

bloxide-spawn/            ← REMOVED (ErasedSpawnFactory, SpawnFactoryFor, SpawnReplyTo deleted)
```

### 3.2 The Factory Trait

```rust
// In bloxide-supervisor-context (folded from bloxide-spawn)

/// A spawn factory creates child actors.
///
/// Implementations are concrete structs (by value, no dyn).
/// The application provides the factory at wiring time.
/// The factory calls R::spawn() internally — the supervisor never does.
pub trait SpawnFactory<R: BloxRuntime> {
    /// Application-specific spawn request enum.
    /// Carries typed reply channels — no type erasure needed.
    type Request: Send + 'static;

    /// Spawn a child actor.
    ///
    /// Called by the supervisor's handle_spawn_request action.
    /// `notify` is the supervisor's child-event mailbox — the factory
    /// passes it to run_supervised_actor so the child can report
    /// lifecycle events (Done, Failed, Reset, etc.) back to the supervisor.
    ///
    /// The factory:
    ///   1. Creates channels (R::channel)
    ///   2. Constructs the child's context
    ///   3. Spawns the child task (R::spawn) with `notify` as the report channel
    ///   4. Sends any typed reply via the request's reply_to field
    ///   5. Returns SpawnOutput for supervisor registration
    fn spawn(
        &self,
        req: Self::Request,
        notify: ActorRef<ChildLifecycleEvent, R>,
    ) -> SpawnOutput<R>;
}
```

### 3.3 The Application's Spawn Types

The application defines its spawn request enum and reply types in its message crate:

```rust
// In the application's message crate (e.g., pool-messages)
pub enum AppSpawnRequest<R: BloxRuntime> {
    Worker {
        task_id: u32,
        reply_to: ActorRef<SpawnedWorker<R>, R>,  // fully typed reply
    },
    // Add more variants for other child types as needed
}

pub struct SpawnedWorker<R: BloxRuntime> {
    pub child_id: ActorId,
    pub domain_ref: ActorRef<WorkerMsg, R>,
    pub ctrl_ref: ActorRef<WorkerCtrl<R>, R>,
}
```

The application defines its concrete factory in its impl crate:

```rust
// In the application's impl crate (e.g., tokio-pool-demo-impl)
pub struct AppSpawnFactory<R: BloxRuntime> {
    pool_ref: ActorRef<PoolMsg, R>,
}

impl<R: BloxRuntime + SpawnCap> SpawnFactory<R> for AppSpawnFactory<R> {
    type Request = AppSpawnRequest<R>;

    fn spawn(&self, req: Self::Request, notify: ActorRef<ChildLifecycleEvent, R>)
        -> SpawnOutput<R>
    {
        match req {
            AppSpawnRequest::Worker { task_id, reply_to } => {
                let child_id = R::alloc_actor_id();
                let (lifecycle_ref, lifecycle_rx) = R::channel::<LifecycleCommand>(child_id, 16);
                let (domain_ref, domain_rx) = R::channel::<WorkerMsg>(child_id, 16);
                let (ctrl_ref, ctrl_rx) = R::channel::<WorkerCtrl<R>>(child_id, 16);

                let ctx = WorkerCtx::new(child_id, self.pool_ref.clone(), ...);
                let machine = StateMachine::<WorkerSpec<R>>::new(ctx);

                R::spawn(async move {
                    run_supervised_actor(
                        machine,
                        (lifecycle_rx, ctrl_rx, domain_rx),
                        child_id,
                        notify,
                    ).await;
                });

                // Send typed reply — no type erasure
                let _ = reply_to.try_send(child_id, SpawnedWorker {
                    child_id,
                    domain_ref: domain_ref.clone(),
                    ctrl_ref: ctrl_ref.clone(),
                });

                SpawnOutput {
                    child_id,
                    lifecycle_ref,
                    policy: Some(SpawnPolicy::Restart { max: 3 }),
                }
            }
        }
    }
}
```

### 3.4 The Supervisor `blox.toml`

```toml
[actor]
name = "Supervisor"

[event]
name = "SupervisorEvent"
generics = "<R: BloxRuntime>"

# Always present: child lifecycle events
[[event.mailboxes]]
variant = "Child"
message = "ChildLifecycleEvent"
message_path = "bloxide_core::lifecycle::ChildLifecycleEvent"

# Always present: control messages (RegisterChild, HealthCheckTick)
[[event.mailboxes]]
variant = "Control"
message = "SupervisorControl"
message_path = "bloxide_supervisor_context::SupervisorControl"

# Dynamic feature only: spawn requests
[[event.mailboxes]]
variant = "Spawn"
message = "F::Request"
message_path = "F"
feature = "dynamic"

[context]
name = "SupervisorCtx"
generics = "<R: BloxRuntime>"
# codegen appends ", #[cfg(feature = \"dynamic\")] F: SpawnFactory<R>"
# when the dynamic feature is enabled

[[context.uses]]
crate = "bloxide_supervisor_context"
trait = "HasChildGroup<R>"
field = "children"
field_type = "ChildGroup<R>"
role = "ctor"

[[context.fields]]
name = "self_id"
ty = "ActorId"
role = "self_id"

[[context.fields]]
name = "child_notify"
ty = "ActorRef<ChildLifecycleEvent, R>"
role = "ctor"

[[context.fields]]
name = "pending"
ty = "ChildAction"
role = "state"

[[context.fields]]
name = "spawn_factory"
ty = "F"
role = "ctor"
feature = "dynamic"

[topology]
# Running state — lifecycle handling (always present)
[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(ChildLifecycleEvent::Done { .. })"
target = "stay"
actions = ["handle_done_or_failed"]
guards = [{ condition = "ctx.pending == ChildAction::BeginShutdown", target = "ShuttingDown" }]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(ChildLifecycleEvent::Failed { .. })"
target = "stay"
actions = ["handle_done_or_failed"]
guards = [{ condition = "ctx.pending == ChildAction::BeginShutdown", target = "ShuttingDown" }]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(ChildLifecycleEvent::Reset { .. })"
target = "stay"
actions = ["handle_reset"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(ChildLifecycleEvent::Started { .. })"
target = "stay"
actions = ["record_started"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(ChildLifecycleEvent::Alive { .. })"
target = "stay"
actions = ["record_alive"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Control(SupervisorControl::RegisterChild(_))"
target = "stay"
actions = ["register_child"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Control(SupervisorControl::HealthCheckTick)"
target = "stay"
actions = ["handle_health_check"]
guards = [{ condition = "ctx.pending == ChildAction::BeginShutdown", target = "ShuttingDown" }]

# Running state — spawn handling (dynamic feature only)
[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Spawn(_)"
target = "stay"
actions = ["handle_spawn_request"]
feature = "dynamic"

# Running state — catch-alls
[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(_)"
target = "stay"

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Control(_)"
target = "stay"

# ShuttingDown state
[[topology.transitions]]
state = "ShuttingDown"
event = "SupervisorEvent::Child(ChildLifecycleEvent::Stopped { .. })"
target = "stay"
actions = ["record_stopped"]
guards = [{ condition = "ctx.all_children_stopped()", target = "reset" }]

[[topology.transitions]]
state = "ShuttingDown"
event = "SupervisorEvent::Child(_)"
target = "stay"

[[topology.transitions]]
state = "ShuttingDown"
event = "SupervisorEvent::Control(_)"
target = "stay"

[[topology.entry]]
state = "Running"
actions = ["start_children"]

[[topology.entry]]
state = "ShuttingDown"
actions = ["stop_all_children"]

[[topology.states]]
name = "Running"
initial = true

[[topology.states]]
name = "ShuttingDown"
```

### 3.5 What the Codegen Produces

The codegen reads the `blox.toml` and emits four files:

#### `generated/ctx.rs`

```rust
#[derive(BloxCtx)]
pub struct SupervisorCtx<
    R: BloxRuntime,
    #[cfg(feature = "dynamic")] F: SpawnFactory<R>,
> {
    pub self_id: ActorId,
    #[provides(HasChildGroup<R>)]
    pub children: ChildGroup<R>,
    pub pending: ChildAction,
    pub child_notify: ActorRef<ChildLifecycleEvent, R>,
    #[cfg(feature = "dynamic")]
    pub spawn_factory: F,
}
```

#### `generated/event.rs`

```rust
pub enum SupervisorEvent<
    R: BloxRuntime,
    #[cfg(feature = "dynamic")] F: SpawnFactory<R>,
> {
    Child(ChildLifecycleEvent),
    Control(SupervisorControl<R>),
    #[cfg(feature = "dynamic")]
    Spawn(F::Request),
    Lifecycle(LifecycleCommand),
}
```

#### `generated/topology.rs`

State enum + handler table macro (same as today, but fully generated from transitions — no `handler_fns`).

#### `generated/spec.rs`

The `MachineSpec` impl with transition tables generated from the TOML transitions using
the `transitions!` macro. Spawn transitions are wrapped in `#[cfg(feature = "dynamic")]`.

The `MachineSpec` impl is generic over `R: BloxRuntime` (without dynamic) or
`<R: BloxRuntime, F: SpawnFactory<R>>` (with dynamic). The codegen emits paired impls:

```rust
// Without dynamic feature
#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> MachineSpec for SupervisorSpec<R> {
    type State = SupervisorState;
    type Event = SupervisorEvent<R>;
    type Ctx = SupervisorCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (
        Rt::Stream<ChildLifecycleEvent>,
        Rt::Stream<SupervisorControl<R>>,
    );
    // ... handler table without spawn rules
}

// With dynamic feature
#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> MachineSpec for SupervisorSpec<R, F> {
    type State = SupervisorState;
    type Event = SupervisorEvent<R, F>;
    type Ctx = SupervisorCtx<R, F>;
    type Mailboxes<Rt: BloxRuntime> = (
        Rt::Stream<ChildLifecycleEvent>,
        Rt::Stream<SupervisorControl<R>>,
        Rt::Stream<F::Request>,
    );
    // ... handler table with spawn rules
}
```

### 3.6 The `handle_spawn_request` Action

```rust
// In bloxide-supervisor-actions (behind #[cfg(feature = "dynamic")])

pub fn handle_spawn_request<R, F>(
    ctx: &mut SupervisorCtx<R, F>,
    ev: &SupervisorEvent<R, F>,
) -> ActionResult
where
    R: BloxRuntime,
    F: SpawnFactory<R>,
{
    if let SupervisorEvent::Spawn(request) = ev {
        // Pass the supervisor's notify channel to the factory.
        // The factory passes it to run_supervised_actor so the child
        // can report lifecycle events back to this supervisor.
        let notify = ctx.child_notify.clone();

        // Call the factory — concrete dispatch, no dyn
        let output = ctx.spawn_factory.spawn(request.clone(), notify);

        // Register the child with the supervisor
        ctx.children.add(output.child_id, output.lifecycle_ref, output.policy);
        ctx.children.start_child(output.child_id, ctx.self_id);
    }
    ActionResult::Ok
}
```

### 3.7 The Full Spawn Flow

```
┌──────────┐                    ┌──────────────┐                  ┌─────────────┐
│   Pool   │                    │  Supervisor  │                  │   Factory   │
│  (blox)  │                    │  (blox)      │                  │ (impl crate)│
└────┬─────┘                    └──────┬───────┘                  └──────┬──────┘
     │                                │                                 │
     │ 1. Create typed reply channel  │                                 │
     │    R::channel::<SpawnedWorker> │                                 │
     │                                │                                 │
     │ 2. SupervisorEvent::Spawn(     │                                 │
     │      AppSpawnRequest::Worker { │                                 │
     │        task_id, reply_to       │                                 │
     │      })                         │                                 │
     │───────────────────────────────>│                                 │
     │                                │                                 │
     │                                │ 3. handle_spawn_request action   │
     │                                │    ctx.spawn_factory.spawn(     │
     │                                │      request, notify)           │
     │                                │────────────────────────────────>│
     │                                │                                 │
     │                                │                                 │ 4. Create channels
     │                                │                                 │    R::channel x3
     │                                │                                 │
     │                                │                                 │ 5. Construct WorkerCtx
     │                                │                                 │
     │                                │                                 │ 6. R::spawn(async { ... })
     │                                │                                 │
     │ <──────────────────────────────────────────────────────────────│ 7. reply_to.try_send(
     │    SpawnedWorker {              │                                 │      SpawnedWorker {
     │      domain_ref, ctrl_ref      │                                 │        domain_ref,
     │    }                             │                                 │        ctrl_ref,
     │                                │                                 │      })
     │                                │                                 │
     │                                │ 8. SpawnOutput {                │
     │                                │      child_id,                  │
     │                                │      lifecycle_ref,             │
     │                                │      policy                     │
     │                                │    }                            │
     │                                │<────────────────────────────────│
     │                                │                                 │
     │                                │ 9. children.add(...)            │
     │                                │    children.start_child(...)    │
     │                                │    (sends Start to child)       │
     │                                │                                 │
     │ 10. Pool has refs, sends DoWork│                                 │
     │───────────────────────────────>│ (to worker, not supervisor)     │
     │                                │                                 │
```

### 3.8 Static vs Dynamic — What the User Sees

**Embassy (static, no `dynamic` feature):**
- `bloxide-supervisor` without `dynamic` feature
- `SupervisorCtx<R>` — 1 generic param, no factory field
- `SupervisorEvent<R>` — no `Spawn` variant
- Supervisor manages static children via `RegisterChild` + lifecycle events
- `R: BloxRuntime` only — no `SpawnCap` needed
- All children declared at compile time, channels created at wiring

**Tokio (dynamic, with `dynamic` feature):**
- `bloxide-supervisor` with `dynamic = ["bloxide-spawn"]`
- `SupervisorCtx<R, F>` — 2 generic params, factory field
- `SupervisorEvent<R, F>` — includes `Spawn(F::Request)` variant
- Application provides concrete `AppSpawnFactory` at wiring time
- `R: BloxRuntime + SpawnCap` — runtime supports task spawning
- Children created at runtime via `SupervisorEvent::Spawn`

### 3.9 What's Hand-Written vs Generated

| Layer | Generated from TOML | Hand-written |
|-------|---------------------|--------------|
| Context struct | ✅ `generated/ctx.rs` | Traits + data types in context crate |
| Event enum | ✅ `generated/event.rs` | Message types in message crate |
| State enum | ✅ `generated/topology.rs` | — |
| Handler tables | ✅ `generated/spec.rs` | — |
| MachineSpec impl | ✅ `generated/spec.rs` | — |
| Action functions | — | ✅ In action crate (bodies only) |
| Factory struct | — | ✅ In impl crate (concrete, by value) |
| Spawn request/reply types | — | ✅ In message crate |

## 4. Codegen Changes Required

### 4.1 `feature` field on TOML entries

Add optional `feature` field to:
- `[[event.mailboxes]]` — conditionally include the mailbox variant
- `[[context.fields]]` — conditionally include the context field
- `[[topology.transitions]]` — conditionally include the transition rule

When `feature = "dynamic"` is present, the codegen wraps the generated item in `#[cfg(feature = "dynamic")]`.

### 4.2 Conditional generic parameters

When any context field or event mailbox has `feature = "dynamic"`, the codegen:
1. Adds `F: SpawnFactory<R>` as a second generic parameter on the context struct and event enum, wrapped in `#[cfg(feature = "dynamic")]`
2. Emits paired `MachineSpec` impls — one without `F` (no dynamic), one with `F` (dynamic)
3. The `SupervisorSpec` type becomes `SupervisorSpec<R>` or `SupervisorSpec<R, F>` depending on feature

### 4.3 Declarative transitions (no `handler_fns`)

The current supervisor uses `handler_fns = ["RUNNING_FNS", "SHUTTING_DOWN_FNS"]` to reference hand-written handler tables. The new design uses declarative `[[topology.transitions]]` entries exclusively. The codegen generates the handler tables from the TOML transitions using the `transitions!` macro (or direct `StateRule` array emission for `#[cfg]` support).

### 4.4 `#[cfg]` on transition rules

The `transitions!` macro produces `&[StateRule { ... }, ...]` arrays. For `#[cfg]` on individual rules, the codegen emits the array directly (not via the macro) with `#[cfg(feature = "dynamic")]` on spawn rules:

```rust
const RUNNING_FNS: StateFns<Self> = StateFns {
    on_entry: &[start_children::<R, SupervisorCtx<R>>],
    on_exit: &[],
    transitions: &[
        StateRule {
            matches: |ev| matches!(ev, SupervisorEvent::Child(ChildLifecycleEvent::Done { .. })),
            actions: &[handle_done_or_failed::<R>],
            guard: |ctx, _, ev| { ... },
        },
        // ... other lifecycle rules ...
        #[cfg(feature = "dynamic")]
        StateRule {
            matches: |ev| matches!(ev, SupervisorEvent::Spawn(_)),
            actions: &[handle_spawn_request::<R, F>],
            guard: |_, _, _| Guard::Stay,
        },
        // ... catch-all rules ...
    ],
};
```

Note: `#[cfg]` on individual array elements is stable since Rust 1.41.

## 5. What Gets Removed

| Removed | Why |
|---------|-----|
| `bloxide-spawn` crate's `ErasedSpawnFactory`, `SpawnFactoryFor` | Replaced by `SpawnFactory<R>` with associated `type Request` — no type erasure |
| `bloxide-spawn` crate's `SpawnReplyTo`, `ErasedReplyTo` | Replaced by typed reply channel in `SpawnFactory::Request` |
| `spec/architecture/15-supervisor-owned-spawning.md` | Superseded by this spec (file deleted) |
| `spec/architecture/16-spawn-service.md` | Superseded — no separate spawn service (file deleted) |
| `spec/architecture/17-spawn-cap-design.md` | Superseded — no type erasure needed (file deleted) |
| Hand-written `SupervisorSpec`, `SupervisorCtx`, handler tables in `supervisor.rs` | Replaced by codegen from `blox.toml` |

## 6. What Stays

| Stays | Where |
|-------|-------|
| `SpawnCap` trait | `bloxide-core` (moved from `bloxide-spawn`) |
| `SpawnOutput<R>`, `SpawnPolicy` | `bloxide-supervisor-context` (folded from `bloxide-spawn`) |
| `SpawnFactory<R>` trait | `bloxide-supervisor-context` (folded from `bloxide-spawn`) |
| `PeerCtrl`, `introduce_peers` | `bloxide-peers` (extracted from `bloxide-spawn`) |
| `ChildGroup<R>`, `ChildPolicy`, etc. | `bloxide-supervisor-context` (new crate, extracted from `bloxide-supervisor`) |
| `SupervisorControl<R>` (RegisterChild, HealthCheckTick) | `bloxide-supervisor-context` |
| `KillCap` | `bloxide-core` (unchanged) |
| Lifecycle types (`ChildLifecycleEvent`, `LifecycleCommand`) | `bloxide-core` (unchanged) |

## 7. Resolved Design Decisions

### Q1: Where does `SpawnCap` live?

**Decision: `bloxide-core`.** It's a runtime capability like `DynamicChannelCap` and `KillCap`. `bloxide-core` already holds these capability traits. `SpawnCap` is `fn spawn(future)` — minimal, no dependencies beyond what `bloxide-core` already has.

### Q2: Does `bloxide-spawn` survive as a separate crate?

**Decision: Folded.** `SpawnFactory<R>`, `SpawnOutput<R>`, `SpawnPolicy` move to `bloxide-supervisor-context` (alongside `ChildGroup`, `ChildPolicy`, etc.). `PeerCtrl` and `introduce_peers` move to a `bloxide-peers` crate (or stay in a minimal `bloxide-spawn` if they're used broadly). The `bloxide-spawn` crate as it exists today is removed — `ErasedSpawnFactory`, `SpawnFactoryFor`, `SpawnReplyTo` are all eliminated.

### Q3: How does the Pool send `Spawn` events to the supervisor?

**Decision: Via `system.toml` wiring.** The Pool holds an `ActorRef<F::Request, R>` (the supervisor's spawn mailbox) as a constructor field, injected at wiring time. The `system.toml` connects the Pool's spawn-ref to the supervisor's spawn mailbox, same as it connects `peer_ref` or `timer_ref` today.

### Q4: Does the Pool still hold `worker_factory`?

**Decision: No.** The factory moves to the supervisor context (`SupervisorCtx.spawn_factory: F`). The Pool sends `Spawn` events and receives typed replies. The Pool's state machine changes: spawning becomes async (send request → wait for reply → send DoWork). The Pool needs a `Spawning` state or pending-reply tracking.

### Q5: How does the factory get the supervisor's notify channel?

**Decision: `child_notify` constructor field on `SupervisorCtx`.**

The supervisor's child-event mailbox (the channel children use to send `ChildLifecycleEvent` back) has a sender side. That sender is stored in `SupervisorCtx` as a constructor field:

```rust
pub struct SupervisorCtx<R: BloxRuntime, #[cfg(feature = "dynamic")] F: SpawnFactory<R>> {
    pub self_id: ActorId,
    #[provides(HasChildGroup<R>)]
    pub children: ChildGroup<R>,
    pub pending: ChildAction,
    pub child_notify: ActorRef<ChildLifecycleEvent, R>,  // for factory
    #[cfg(feature = "dynamic")]
    pub spawn_factory: F,
}
```

The `handle_spawn_request` action passes `ctx.child_notify.clone()` to `ctx.spawn_factory.spawn(req, notify)`. The factory passes it to `run_supervised_actor`, which uses it to report lifecycle events back to the supervisor.

**Why not capture in the factory at wiring time?** Channels are created and destroyed throughout runtime with dynamic spawning. The supervisor's notify channel is created when the supervisor task starts, not at system wiring time. If the supervisor is reset, it gets a new context with a new notify channel. The factory must use the current one, not a stale capture.

**Why not on `ChildGroup`?** `ChildGroup` is pure data — it holds `lifecycle_ref`s for *sending commands to children* but doesn't hold a ref for *receiving events from children*. Adding a notify ref would couple a data structure to runtime infrastructure. The notify ref is a supervisor-level concern, not a child-registry concern.

The `blox.toml` entry:

```toml
[[context.fields]]
name = "child_notify"
ty = "ActorRef<ChildLifecycleEvent, R>"
role = "ctor"
```

The codegen creates the channel at wiring time, injects the sender into the supervisor context (as `child_notify`), and feeds the receiver into the supervisor's mailboxes (as the `Child` event stream). This is the same pattern as `self_ref` — the sender goes to the context, the receiver goes to the event loop.

## 8. Migration Path

1. **Add `SpawnCap` to `bloxide-core`** — move the trait from `bloxide-spawn` to `bloxide-core` alongside `DynamicChannelCap` and `KillCap`.
2. **Create `bloxide-supervisor-context` crate** — extract `ChildGroup`, `ChildPolicy`, `GroupShutdown`, `RestartStrategy`, `ChildAction`, `HasChildGroup`, `SupervisorControl`, `RegisterChild` from `bloxide-supervisor`. Fold in `SpawnFactory<R>`, `SpawnOutput<R>`, `SpawnPolicy` from `bloxide-spawn`.
3. **Create `bloxide-supervisor-actions` crate** — extract action functions from `supervisor.rs`. Add `handle_spawn_request` behind `#[cfg(feature = "dynamic")]`.
4. **Create `bloxide-peers` crate** — extract `PeerCtrl` and `introduce_peers` from `bloxide-spawn`.
5. **Update `bloxide-supervisor/blox.toml`** — add declarative transitions, `context.uses`, `child_notify` field, feature-gated fields/mailboxes/transitions.
6. **Update codegen** — support `feature` field on TOML entries, conditional generic parameters, paired `MachineSpec` impls, `#[cfg]` on transition rules, `child_notify` channel creation at wiring time.
7. **Delete hand-written `supervisor.rs`** — everything is now generated.
8. **Remove `bloxide-spawn` crate** — `ErasedSpawnFactory`, `SpawnFactoryFor`, `SpawnReplyTo` deleted. `SpawnCap` moved to `bloxide-core`. `SpawnFactory`/`SpawnOutput`/`SpawnPolicy` moved to `bloxide-supervisor-context`. `PeerCtrl`/`introduce_peers` moved to `bloxide-peers`.
9. **Update pool demo** — move factory to supervisor, add `AppSpawnRequest`/`SpawnedWorker` types to message crate, add `AppSpawnFactory` to impl crate, update Pool state machine for async spawn (send request → wait for reply → send DoWork), wire Pool's spawn-ref to supervisor's spawn mailbox in `system.toml`.
10. **Remove specs 15, 16, 17** — superseded by this spec.
