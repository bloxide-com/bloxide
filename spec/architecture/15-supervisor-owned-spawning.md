# Supervisor-Owned Dynamic Spawning

This document describes the architecture for supervisor-mediated dynamic actor creation, where all spawning flows through the supervisor rather than individual bloxes spawning directly.

## Architecture Principle

The supervisor sits **above all bloxes** and owns all actor lifecycles. This principle applies to both static wiring (Embassy, Tokio, TestRuntime) and dynamic spawning (Tokio, TestRuntime only).

```
┌─────────────────────────────────────────────────────┐
│                    RUNTIME                          │
│  (Tokio/Embassy - provides capabilities)            │
└─────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────┐
│                  SUPERVISOR                         │
│  - Owns ALL actor lifecycles                        │
│  - Sends LifecycleCommand to children               │
│  - Receives ChildLifecycleEvent from children       │
│  - Static children: works everywhere                │
│  - Dynamic spawning: requires DynamicActorSupport   │
└─────────────────────────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
    ┌──────────┐    ┌──────────┐    ┌──────────┐
    │   Pool   │    │   Ping   │    │   Pong   │
    │(requires │    │ (works   │    │ (works   │
    │ Dynamic) │    │everywhere)│   │everywhere)│
    └──────────┘    └──────────┘    └──────────┘
          │
          │ (workers are ALSO supervised children)
          ▼
    ┌──────────┐
    │  Worker  │
    │ (works   │
    │everywhere)│
    └──────────┘
```

**Key insight**: Pool does NOT spawn workers directly. The supervisor does. Pool decides "I need a worker" and the supervisor fulfills that via the factory it was configured with.

---

## Trait Consolidation: DynamicActorSupport

### The Unified Capability

`DynamicActorSupport` is the **single trait** for dynamic actor capability. It combines ID allocation, channel creation, and task spawning into one unified trait.

```rust
// In bloxide-core/src/capability.rs

/// Marker + capability for runtimes that support dynamic actors.
/// 
/// Combines ID allocation, channel creation, and task spawning into
/// a single unified capability. A runtime either supports all of this
/// or none of it.
/// 
/// - TokioRuntime: implements this (native support)
/// - EmbassyRuntime: does NOT implement this (static wiring only)
/// - TestRuntime: implements this (test-friendly)
/// 
/// For static wiring patterns (Embassy), use StaticChannelCap instead.
pub trait DynamicActorSupport: BloxRuntime {
    /// Allocate a new actor ID at runtime.
    fn alloc_actor_id() -> ActorId;
    
    /// Create a channel for a dynamically allocated actor.
    fn channel<M: Send + 'static>(
        id: ActorId,
        capacity: usize,
    ) -> (ActorRef<M, Self>, Self::Stream<M>);
    
    /// Spawn a future as an independent task.
    fn spawn(future: impl Future<Output = ()> + Send + 'static);
}
```

This replaces the previous two separate traits:
- `DynamicChannelCap` — ID allocation and channel creation
- `SpawnCap` — task spawning

A runtime either supports all dynamic actor capabilities or none. There is no partial support.

### Runtime Support Matrix

| Runtime | DynamicActorSupport | Notes |
|---------|---------------------|-------|
| `TokioRuntime` | ✅ Yes | Native dynamic spawning via `tokio::spawn` |
| `EmbassyRuntime` | ❌ No | Compile-time static wiring only |
| `TestRuntime` | ✅ Yes | Test-friendly dynamic spawning |

Embassy has no dynamic spawning by design — `#[embassy_executor::task]` functions must be declared at compile time.

### Compile-Time Guarantees

Bloxes that require dynamic spawning bound on `DynamicActorSupport`:

```rust
// Pool REQUIRES dynamic spawning - won't compile for Embassy
impl<R: BloxRuntime + DynamicActorSupport> MachineSpec for PoolSpec<R> { ... }

// Bloxes that work everywhere use the normal bound
impl<R: BloxRuntime> MachineSpec for PongSpec<R> { ... }
impl<R: BloxRuntime> MachineSpec for WorkerSpec<R> { ... }
```

The compiler rejects any attempt to use `PoolSpec<EmbassyRuntime>` because Embassy doesn't implement `DynamicActorSupport`.

---

## Two Spawning Flows

### Static Children (All Runtimes)

Static children are created at wiring time, before the executor starts.

```
Wiring Layer:
  - Creates ChildGroupBuilder
  - Creates each child's channels, context, machine
  - Calls spawn_child! which:
    - Creates lifecycle channel
    - Registers in ChildGroup
    - Spawns run_supervised_actor via runtime
  - Spawns supervisor with the ChildGroup
```

Works on: **All runtimes** (Tokio, Embassy, TestRuntime)

### Dynamic Children (DynamicActorSupport Only)

Dynamic children are created at runtime, after the executor has started.

```
Wiring Layer:
  - Creates supervisor with ChildGroup AND a "dynamic child factory map"
  - Factories are keyed by child type (e.g., "worker")

Pool (at runtime):
  - Sends message to supervisor: "SpawnChild { type: 'worker', task_id: 0 }"
  
Supervisor (if R: DynamicActorSupport):
  - Looks up factory for 'worker' type
  - Calls factory to get (ctx, mailboxes, id)
  - Creates lifecycle channel via R::channel()
  - Registers in ChildGroup with policy
  - Spawns via R::spawn(run_supervised_actor(...))
  - Optional: replies to pool with ActorRef
```

Works on: **Only runtimes implementing DynamicActorSupport** (Tokio, TestRuntime)

---

## Supervisor Control Events

### SpawnChild Event

Actors request dynamic child creation via the `SupervisorControl::SpawnChild` event:

```rust
// In bloxide-supervisor/src/control.rs

pub enum SupervisorControl<R: BloxRuntime> {
    // Existing events
    RegisterChild(RegisterChild<R>),
    HealthCheckTick,
    
    // NEW: Request dynamic child spawn (requires R: DynamicActorSupport)
    SpawnChild {
        child_type: ChildType,      // e.g., ChildType::Worker
        params: SpawnParams,        // e.g., task_id
        reply_to: Option<ActorRef<SpawnReply, R>>,  // optional reply
    },
}

pub enum SpawnReply {
    Spawned { child_id: ActorId },
    SpawnFailed { reason: SpawnError },
}

pub enum SpawnError {
    UnsupportedRuntime,  // Embassy doesn't support dynamic spawning
    FactoryNotFound,
    ChannelCreationFailed,
}

pub enum ChildType {
    Worker,
    // ... other dynamic child types as needed
}
```

### Supervisor Handling

The supervisor's event handler processes `SpawnChild` only when the runtime implements `DynamicActorSupport`:

```rust
impl<R: BloxRuntime> MachineSpec for SupervisorSpec<R> {
    // Static children, health checks, lifecycle management - all work
    
    // SpawnChild event handling:
    //   - On Tokio/TestRuntime (R: DynamicActorSupport): handler exists, spawns dynamically
    //   - On Embassy (no DynamicActorSupport): event is ignored or returns error
}
```

---

## Factory Injection Pattern

### Factories Return Construction Data

Factories do NOT spawn actors. They return the construction data needed to spawn:

```rust
// Factory returns (context, mailboxes, id) as type-erased boxes
pub trait DynamicChildFactory {
    fn create(&self, params: SpawnParams) -> (Box<dyn Any>, Box<dyn Any>, ActorId);
}

// Concrete factory example
fn create_worker_factory<R: BloxRuntime + DynamicActorSupport>(
    pool_ref: ActorRef<PoolMsg, R>
) -> impl DynamicChildFactory {
    move |params: SpawnParams| {
        let SpawnParams::Worker { task_id } = params else { /* error */ };
        
        let worker_id = R::alloc_actor_id();
        let (domain_ref, ctrl_ref, mailboxes) = create_worker_channels::<R>(worker_id);
        let ctx = WorkerCtx::new(worker_id, pool_ref.clone(), ctrl_ref, task_id);
        
        (Box::new(ctx), Box::new(mailboxes), worker_id)
    }
}
```

The supervisor unboxes the construction data and performs the actual spawn via `DynamicActorSupport::spawn()`.

### Supervisor Factory Registry

The supervisor context stores registered factories:

```rust
pub struct SupervisorCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    
    #[provides(HasChildGroup<R>)]
    pub children: ChildGroup<R>,
    
    // Factory registry for dynamic child types
    // Only used when R: DynamicActorSupport
    pub factories: HashMap<ChildType, Box<dyn DynamicChildFactory>>,
}
```

---

## Application Wiring

### Supervisor with Dynamic Factories

```rust
// Create supervisor with factory registry
let sup_ctx = SupervisorCtx::new(sup_id, child_group)
    .with_factory(ChildType::Worker, make_worker_factory(pool_ref));

// Pool needs reference to supervisor's control channel
let pool_ctx = PoolCtx::new(
    pool_id, 
    pool_ref, 
    sup_control_ref.clone()  // Reference to supervisor's control channel
);
```

### Pool Requesting Worker Spawn

```rust
// Pool action - does NOT spawn directly
pub fn request_worker_spawn<R>(ctx: &mut PoolCtx<R>, task_id: u32)
where R: BloxRuntime
{
    ctx.supervisor_control.try_send(ctx.self_id, 
        SupervisorControl::SpawnChild {
            child_type: ChildType::Worker,
            params: SpawnParams::Worker { task_id },
            reply_to: None,  // Pool tracks via WorkDone messages
        }
    );
}
```

---

## Benefits

### Consistency

All actors use the same supervision model:
- Same lifecycle commands (`Start`, `Reset`, `Stop`)
- Same health monitoring
- Same restart policies
- Same failure handling

### Safety

- Health checks work for dynamically spawned children
- Restart policies apply to dynamic children
- Supervisor knows about ALL children
- No orphaned actors

### Compile-Time Guarantees

- Bloxes requiring dynamic spawning won't compile for Embassy
- No runtime surprises about missing capability
- Clear error messages at compile time

### Separation of Concerns

- **Pool**: decides *when* to spawn (business logic)
- **Supervisor**: decides *how* to spawn (lifecycle management)
- **Factory**: knows *what* to construct (domain details)

---

## Invariant

> **Supervisor owns all spawning** — Blox crates never call `R::spawn()` directly. The supervisor sits above all bloxes and handles lifecycle. Dynamic child creation flows through `SupervisorControl::SpawnChild` requests, which only work on runtimes implementing `DynamicActorSupport`.
>
> Bloxes that require dynamic spawning should bound on `DynamicActorSupport`:
> ```rust
> impl<R: BloxRuntime + DynamicActorSupport> MachineSpec for PoolSpec<R> { ... }
> ```
> This ensures compile-time rejection on Embassy (static wiring only).
>
> `DynamicActorSupport` unifies ID allocation, channel creation, and task spawning — a runtime supports all of these or none. There are no separate traits for these capabilities.

---

## Related Documents

- [00-layered-architecture.md](00-layered-architecture.md) — Three-layer principle, two-tier traits
- [08-supervision.md](08-supervision.md) — Supervisor mechanics and policies
- [11-dynamic-actors.md](11-dynamic-actors.md) — Dynamic spawning overview
- [12-action-crate-pattern.md](12-action-crate-pattern.md) — Factory injection via action crates

---

## Spawn Reply Mechanism

When a requester (e.g., Pool) needs refs to the spawned child, it provides a 
typed reply channel. This enables communication between parent and child actors
while keeping the supervisor unaware of concrete message types.

### Type Erasure Strategy

```
┌────────────────────────────────────────────────────────────────────┐
│                    TYPE ERASURE STRATEGY                           │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│  1. REQUESTER (e.g., Pool) knows SpawnedWorker<WorkerMsg, R>      │
│     │                                                              │
│     ▼                                                              │
│  2. REQUESTER creates typed reply channel:                         │
│     let (reply_ref, reply_rx) = R::channel::<SpawnedWorker<R>>()   │
│     │                                                              │
│     ▼                                                              │
│  3. REQUESTER wraps in SpawnReplyTo (type erasure):                │
│     let reply_to = SpawnReplyTo::from_typed(reply_ref)             │
│     │                                                              │
│     ▼                                                              │
│  4. REQUESTER sends SpawnChild with type-erased reply_to           │
│     │                                                              │
│     ▼                                                              │
│  5. FACTORY (in impl crate) knows SpawnedWorker<R>                │
│     │                                                              │
│     ▼                                                              │
│  6. FACTORY downcasts: reply_to.into_sender::<SpawnedWorker<R>>() │
│     │                                                              │
│     ▼                                                              │
│  7. FACTORY sends typed SpawnedWorker { domain_ref, ctrl_ref }     │
│     │                                                              │
│     ▼                                                              │
│  8. REQUESTER receives typed refs, stores them, uses them          │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

### Requester Code

```rust
// Requester creates typed reply
let (reply_ref, reply_rx) = R::channel::<SpawnedWorker<R>>(self_id, 4);

// Type-erased for transport through supervisor
let reply_to = SpawnReplyTo::from_typed(reply_ref);

// Send spawn request
ctx.supervisor_control.try_send(self_id,
    SupervisorControl::SpawnChild {
        child_type: ChildType::Worker,
        params: SpawnParams::Worker { task_id },
        reply_to: Some(reply_to),
    }
);

// Track pending reply
ctx.pending_spawn_replies.push((reply_rx, task_id));
ctx.pending_spawns += 1;
```

### Factory Code

The factory downcasts and sends the typed reply:

```rust
// In factory implementation
impl<R: BloxRuntime + DynamicActorSupport> DynamicChildFactory<R> for WorkerFactory<R> {
    fn spawn(
        &self,
        child_id: ActorId,
        lifecycle_ref: ActorRef<LifecycleCommand, R>,
        lifecycle_rx: R::Stream<LifecycleCommand>,
        params: SpawnParams,
        reply_to: Option<SpawnReplyTo<R>>,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
    ) -> Option<FactorySpawnResult> {
        // 1. Downcast reply_to to typed sender
        let sender = reply_to?.into_sender::<SpawnedWorker<R>>()?;
        
        // 2. Create child's domain channels
        let (domain_ref, domain_rx) = R::channel::<WorkerMsg>(child_id, 16);
        let (ctrl_ref, ctrl_rx) = R::channel::<WorkerCtrl<R>>(child_id, 4);
        
        // 3. Create child context
        let worker_ctx = WorkerCtx::new(child_id, self.pool_ref.clone());
        worker_ctx.set_task_id(task_id);
        
        // 4. Spawn child task
        R::spawn(async {
            run_supervised_actor(machine, (lifecycle_rx, ctrl_rx, domain_rx), 
                                 child_id, supervisor_notify).await;
        });
        
        // 5. Send typed reply with refs
        sender.try_send(Envelope(supervisor_notify.actor_id(), SpawnedWorker {
            child_id,
            domain_ref: domain_ref.clone(),
            ctrl_ref: ctrl_ref.clone(),
        }));
        
        Some(FactorySpawnResult { policy: Some(ChildPolicy::Restart { max: 3 }) })
    }
}
```

### Requester Polling

The requester polls for replies in `on_entry` or a tick handler:

```rust
// In requester's Active state on_entry
fn on_entry(ctx: &mut PoolCtx<R>) {
    ctx.poll_spawn_replies();
}

// In context impl
impl<R: BloxRuntime> PoolCtx<R> {
    pub fn poll_spawn_replies(&mut self) {
        while let Some((rx, task_id)) = self.pending_spawn_replies.first_mut() {
            if let Some(Envelope(_, spawned)) = rx.try_recv() {
                // Store refs
                self.worker_refs.push(spawned.domain_ref.clone());
                self.worker_ctrls.push(spawned.ctrl_ref.clone());
                self.pending += 1;
                self.pending_spawns -= 1;
                
                // Do peer introduction
                introduce_new_worker(self);
                
                // Send work to child
                spawned.domain_ref.try_send(self.self_id, 
                    WorkerMsg::DoWork(DoWork { task_id }));
            }
        }
    }
}
```

### Design Rationale

1. **Supervisor doesn't know message types** - Type erasure via `SpawnReplyTo` allows 
   heterogeneous spawn replies to flow through the supervisor without it understanding
   the concrete types.

2. **Factory knows concrete types** - The impl crate imports the concrete blox types, 
   so it can work with properly typed refs and downcast safely.

3. **Requester receives typed refs** - Downcasting succeeds because the requester 
   and factory agree on the reply type through shared message/action crate definitions.

4. **Two-phase async** - Spawn is asynchronous; requester must poll for replies. 
   This matches the actor model where timing is uncertain and lets the requester
   control message ordering (e.g., peer introduction before DoWork).

5. **Works for all parent/child pairs** - The pattern is generic and works for
   any relationship where the parent needs to communicate with spawned children.
