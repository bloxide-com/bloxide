# Spawn Service: External Capability for Dynamic Spawning

This document describes the architecture for dynamic actor spawning as an external
capability service, following the same pattern as `TimerService`.

## Design Principle

Spawning is a **runtime capability**, not a supervisor responsibility. The supervisor
owns lifecycle management; a separate `SpawnService` owns actor creation.

```
┌─────────────────────────────────────────────────────────────┐
│                     RUNTIME LAYER                           │
│  (Provides SpawnService if DynamicActorSupport is impl'd)   │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    SPAWN SERVICE                            │
│  - Subscribes to supervisor control channel                 │
│  - Holds factory registry (keyed by ChildType)             │
│  - On SpawnChild: calls factory, spawns actor               │
│  - Sends RegisterChild to supervisor with new lifecycle_ref │
│  - Optionally sends typed reply to requester                │
└─────────────────────────────────────────────────────────────┘
                          │
          ┌───────────────┴───────────────┐
          ▼                               ▼
┌─────────────────────┐         ┌─────────────────────┐
│     SUPERVISOR      │         │   REQUESTER (Pool)  │
│  - Emits SpawnChild │         │  - Gets typed reply │
│  - Receives         │         │    with child refs  │
│    RegisterChild    │         │  - Or polls reply   │
│  - Manages lifecycle│         │    channel async    │
└─────────────────────┘         └─────────────────────┘
```

---

## Message Flow

### Flow 1: Spawn Request Without Reply

Used when requester doesn't need refs to the spawned child.

```
Pool                     Supervisor              SpawnService
  │                           │                        │
  │ SpawnChild{Worker,       │                        │
  │   task_id: 42,           │                        │
  │   reply_to: None}        │                        │
  │ ──────────────────────▶  │                        │
  │                          │  (event absorbed,       │
  │                          │   SpawnService sees it) │
  │                          │                        │
  │                          │         SpawnChild seen on shared control channel
  │                          │  ◀─────────────────────│
  │                          │                        │
  │                          │     factory.spawn()    │
  │                          │     R::spawn(...)      │
  │                          │                        │
  │                          │  RegisterChild{        │
  │                          │    id, lifecycle_ref,  │
  │                          │    policy}             │
  │                          │  ◀─────────────────────│
  │                          │                        │
  │                          │  (register child,      │
  │                          │   send Start)          │
```

### Flow 2: Spawn Request With Typed Reply

Used when requester needs refs to communicate with the spawned child.

```
Pool                     Supervisor              SpawnService
  │                           │                        │
  │ 1. Create reply channel   │                        │
  │    R::channel::<SpawnedWorker<R>>()                │
  │                           │                        │
  │ SpawnChild{Worker,        │                        │
  │   task_id: 42,            │                        │
  │   reply_to: Some(...)}    │                        │
  │ ──────────────────────▶  │                        │
  │                           │                        │
  │                           │         SpawnChild seen
  │                           │  ◀─────────────────────│
  │                           │                        │
  │                           │     factory.spawn()    │
  │                           │     R::spawn(...)      │
  │                           │                        │
  │                           │      SpawnedWorker{    │
  │     ◀──────────────────────────────────────────    │
  │        child_id, domain_ref, ctrl_ref }            │
  │                           │                        │
  │                           │  RegisterChild{...}    │
  │                           │  ◀─────────────────────│
```

---

## Trait Definitions

### SpawnFactory Trait

Defined in `bloxide-spawn` crate (new standard library crate).

```rust
// In bloxide-spawn/src/factory.rs

use bloxide_core::{
    capability::{BloxRuntime, DynamicActorSupport},
    messaging::{ActorId, ActorRef, Envelope},
};
use bloxide_supervisor::{
    SpawnParams, SpawnReplyTo, ChildLifecycleEvent, 
    ChildPolicy, LifecycleCommand,
};

/// Result of a factory spawn operation.
pub struct SpawnOutput<R: BloxRuntime> {
    /// The spawned child's ID.
    pub child_id: ActorId,
    /// Lifecycle ref for supervisor to manage the child.
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    /// Optional domain ref to send in typed reply.
    /// Use `set_domain_ref()` to populate.
    pub domain_ref: Option<ActorRef<(), R>>,  // Type-erased
    /// Optional ctrl ref to send in typed reply.
    pub ctrl_ref: Option<ActorRef<(), R>>,    // Type-erased
    /// Optional policy override.
    pub policy: Option<ChildPolicy>,
}

/// Factory trait for spawning children.
/// 
/// Implementations are provided by impl crates and contain
/// all the knowledge about how to construct a specific child type.
/// 
/// The SpawnService calls this trait; the factory does the actual
/// channel creation, context construction, and task spawning.
pub trait SpawnFactory<R: BloxRuntime + DynamicActorSupport>: Send + Sync {
    /// Spawn a child actor.
    /// 
    /// # Arguments
    /// * `supervisor_id` - The supervisor's actor ID (for logging)
    /// * `supervisor_notify` - Where to send lifecycle events
    /// * `params` - Spawn parameters (e.g., task_id for Worker)
    /// * `reply_to` - Optional typed reply channel
    /// 
    /// # Returns
    /// `Some(output)` on success, `None` on failure.
    /// 
    /// # Responsibilities
    /// 1. Allocate child ID via `R::alloc_actor_id()`
    /// 2. Create lifecycle channel via `R::channel::<LifecycleCommand>()`
    /// 3. Create domain channels for the child
    /// 4. Construct child context and state machine
    /// 5. Spawn child task via `R::spawn(run_supervised_actor(...))`
    /// 6. Send typed reply via `reply_to` if provided
    fn spawn(
        &self,
        supervisor_id: ActorId,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: SpawnParams,
        reply_to: Option<SpawnReplyTo<R>>,
    ) -> Option<SpawnOutput<R>>;
}
```

### SpawnService Trait

The runtime capability trait, similar to `TimerService`.

```rust
// In bloxide-core/src/capability.rs (or bloxide-spawn)

use bloxide_supervisor::{SupervisorControl, RegisterChild, ChildType};
use bloxide_spawn::SpawnFactory;

/// Capability to spawn actors dynamically.
/// 
/// Runtimes that implement `DynamicActorSupport` should also implement
/// this trait. The implementation provides a `SpawnService` that:
/// - Subscribes to supervisor control channels
/// - Holds a registry of factories
/// - Processes `SpawnChild` requests
/// 
/// This is a **Tier 2** capability trait.
pub trait SpawnCap: BloxRuntime + DynamicActorSupport {
    /// Register a factory for a child type.
    /// 
    /// Called at wiring time before the runtime starts.
    fn register_spawn_factory<M>(
        child_type: ChildType,
        factory: impl SpawnFactory<Self> + 'static,
    );
    
    /// Start listening for spawn requests on a supervisor's control channel.
    /// 
    /// The SpawnService subscribes to the control channel and processes
    /// `SpawnChild` events as they arrive.
    /// 
    /// Returns a handle that can be used to stop the spawn service.
    fn spawn_service_listen(
        supervisor_control_rx: Self::Stream<SupervisorControl<Self>>,
        supervisor_control_tx: ActorRef<SupervisorControl<Self>, Self>,
    ) -> SpawnServiceHandle;
}

/// Opaque handle to a running spawn service.
pub struct SpawnServiceHandle { /* ... */ }
```

---

## SpawnService Implementation

### Architecture

The `SpawnService` is itself an actor (or a simple async task) that:

1. **Receives SpawnChild requests** from the supervisor control channel
2. **Looks up the factory** for the requested ChildType
3. **Calls factory.spawn()** to create the child
4. **Sends RegisterChild** back to the supervisor
5. **Sends typed reply** if requested

### Implementation (Tokio Example)

```rust
// In bloxide-tokio/src/spawn_service.rs

use bloxide_core::{DynamicActorSupport, BloxRuntime, ActorRef};
use bloxide_supervisor::{SupervisorControl, RegisterChild, ChildType, ChildLifecycleEvent};
use bloxide_spawn::{SpawnFactory, SpawnOutput, spawn_service_handle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Global factory registry.
/// Uses std::sync because it's accessed from multiple tasks.
static SPAWN_FACTORIES: once_cell::sync::Lazy<
    Arc<Mutex<HashMap<ChildType, Box<dyn SpawnFactory<TokioRuntime>>>>>
> = once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

impl SpawnCap for TokioRuntime {
    fn register_spawn_factory<M>(
        child_type: ChildType,
        factory: impl SpawnFactory<Self> + 'static,
    ) {
        let mut registry = SPAWN_FACTORIES.lock().unwrap();
        registry.insert(child_type, Box::new(factory));
    }
    
    fn spawn_service_listen(
        supervisor_control_rx: Self::Stream<SupervisorControl<Self>>,
        supervisor_control_tx: ActorRef<SupervisorControl<Self>, Self>,
    ) -> SpawnServiceHandle {
        let factories = SPAWN_FACTORIES.clone();
        
        let handle = tokio::spawn(async move {
            use futures::StreamExt;
            
            let mut rx = supervisor_control_rx;
            
            while let Some(event) = rx.next().await {
                if let SupervisorControl::SpawnChild { 
                    child_type, 
                    params, 
                    reply_to 
                } = event.payload() {
                    
                    // Look up factory
                    let factory = {
                        let registry = factories.lock().unwrap();
                        registry.get(child_type).cloned()
                    };
                    
                    let Some(factory) = factory else { continue };
                    
                    // Get supervisor_notify from context or pass through
                    // (This requires design decision - see alternatives below)
                    
                    // Call factory
                    if let Some(output) = factory.spawn(
                        event.sender(),
                        /* supervisor_notify */,
                        params.clone(),
                        reply_to.clone(),
                    ) {
                        // Register child with supervisor
                        let register = RegisterChild {
                            id: output.child_id,
                            lifecycle_ref: output.lifecycle_ref,
                            policy: output.policy.unwrap_or(ChildPolicy::Stop),
                        };
                        
                        supervisor_control_tx.try_send(
                            TokioRuntime::actor_id(),  // spawn service ID
                            SupervisorControl::RegisterChild(register),
                        );
                    }
                }
            }
        });
        
        SpawnServiceHandle { handle }
    }
}

pub struct SpawnServiceHandle {
    handle: tokio::task::JoinHandle<()>,
}
```

### Design Question: supervisor_notify

The factory needs `supervisor_notify` to pass to `run_supervised_actor()`. Options:

**Option A: Pass in SpawnChild event**
```rust
SupervisorControl::SpawnChild {
    child_type,
    params,
    reply_to,
    supervisor_notify,  // Add this field
}
```

**Option B: SpawnService receives it separately at initialization**
```rust
fn spawn_service_listen(
    supervisor_control_rx: Self::Stream<SupervisorControl<Self>>,
    supervisor_control_tx: ActorRef<SupervisorControl<Self>, Self>,
    supervisor_notify: ActorRef<ChildLifecycleEvent, Self>,  // Add this
) -> SpawnServiceHandle;
```

**Recommended: Option B** - cleaner separation, supervisor_notify is a "wiring artifact" not part of each spawn request.

---

## Factory Implementation Example

```rust
// In tokio-pool-demo-impl/src/lib.rs

use bloxide_core::{DynamicActorSupport, StateMachine, ActorRef, ActorId};
use bloxide_supervisor::{SpawnParams, SpawnReplyTo, ChildLifecycleEvent, ChildPolicy, LifecycleCommand};
use bloxide_spawn::{SpawnFactory, SpawnOutput};
use bloxide_tokio::TokioRuntime;
use worker_blox::{WorkerCtx, WorkerSpec, WorkerMsg};
use pool_messages::PoolMsg;

pub struct WorkerFactory {
    pool_ref: ActorRef<PoolMsg, TokioRuntime>,
}

impl WorkerFactory {
    pub fn new(pool_ref: ActorRef<PoolMsg, TokioRuntime>) -> Self {
        Self { pool_ref }
    }
}

impl SpawnFactory<TokioRuntime> for WorkerFactory {
    fn spawn(
        &self,
        supervisor_id: ActorId,
        supervisor_notify: ActorRef<ChildLifecycleEvent, TokioRuntime>,
        params: SpawnParams,
        reply_to: Option<SpawnReplyTo<TokioRuntime>>,
    ) -> Option<SpawnOutput<TokioRuntime>> {
        let SpawnParams::Worker { task_id } = params else { return None };
        
        // 1. Allocate ID
        let child_id = TokioRuntime::alloc_actor_id();
        
        // 2. Create lifecycle channel
        let (lifecycle_ref, lifecycle_rx) = TokioRuntime::channel::<LifecycleCommand>(child_id, 16);
        
        // 3. Create domain channels
        let (domain_ref, domain_rx) = TokioRuntime::channel::<WorkerMsg>(child_id, 16);
        
        // 4. Create context
        let ctx = WorkerCtx::new(child_id, self.pool_ref.clone(), task_id);
        let machine = StateMachine::<WorkerSpec<TokioRuntime>>::new(ctx);
        
        // 5. Spawn supervised actor
        TokioRuntime::spawn(run_supervised_actor(
            machine,
            (lifecycle_rx, domain_rx),
            child_id,
            supervisor_notify,
        ));
        
        // 6. Send typed reply if provided
        if let Some(reply) = reply_to {
            if let Some(sender) = reply.get_sender::<SpawnedWorker<TokioRuntime>>() {
                let _ = sender.try_send(Envelope(supervisor_id, SpawnedWorker {
                    child_id,
                    domain_ref: domain_ref.clone(),
                }));
            }
        }
        
        Some(SpawnOutput {
            child_id,
            lifecycle_ref,
            policy: Some(ChildPolicy::Restart { max: 3 }),
        })
    }
}
```

---

## Wiring Example

```rust
// In examples/tokio-pool-demo.rs

use bloxide_tokio::TokioRuntime;
use bloxide_supervisor::{SupervisorCtx, ChildGroup, SupervisorControl};
use bloxide_spawn::SpawnCap;
use tokio_pool_demo_impl::WorkerFactory;

#[tokio::main]
async fn main() {
    // 1. Create supervisor channels
    let sup_id = TokioRuntime::alloc_actor_id();
    let (sup_control_ref, sup_control_rx) = TokioRuntime::channel::<SupervisorControl<TokioRuntime>>(sup_id, 64);
    let (sup_notify_ref, sup_notify_rx) = TokioRuntime::channel::<ChildLifecycleEvent>(sup_id, 64);
    
    // 2. Register factories BEFORE starting spawn service
    let worker_factory = WorkerFactory::new(pool_ref);
    TokioRuntime::register_spawn_factory(ChildType::Worker, worker_factory);
    
    // 3. Start spawn service (listens to supervisor control channel)
    let _spawn_handle = TokioRuntime::spawn_service_listen(
        sup_control_rx,
        sup_control_ref.clone(),
        sup_notify_ref.clone(),
    );
    
    // 4. Create supervisor (no factory field needed!)
    let child_group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let ctx = SupervisorCtx::new(sup_id, child_group, sup_notify_ref);
    let supervisor = StateMachine::<SupervisorSpec<TokioRuntime>>::new(ctx);
    
    // 5. Spawn supervisor
    TokioRuntime::spawn(run_supervised_actor(
        supervisor,
        (sup_notify_rx, sup_control_rx_dup),  // Wait, we need another rx...
    ));
    
    // ... rest of wiring
}
```

### Note: Channel Sharing

The SpawnService and Supervisor both need to receive from the control channel.
Options:

1. **Shared channel** - Both poll the same receiver (requires `Stream::split()`)
2. **Broadcast** - Use a broadcast channel that both can subscribe to
3. **SpawnService intercepts first** - Supervisor only gets non-SpawnChild events

**Recommended: Option 3** - SpawnService processes the control stream, forwards non-spawn events to supervisor via internal channel.

---

## No-Spawn Runtime (Embassy)

Embassy doesn't implement `DynamicActorSupport`, so:

1. No `SpawnCap` implementation
2. No spawn service started
3. `SpawnChild` events sit in the control channel (or are absorbed if no match)

The supervisor works exactly the same - static children are registered at compile time.

---

## Summary

| Component | Responsibility |
|-----------|---------------|
| `Supervisor` | Owns lifecycle, emits `SpawnChild`, receives `RegisterChild` |
| `SpawnService` | Subscribes to control channel, calls factory, spawns actors |
| `SpawnFactory` | Knows how to create a specific child type (impl crate) |
| `SpawnCap` | Runtime trait for spawn capability (like `TimerService`) |

**Benefits:**
1. Supervisor has NO spawning logic - pure lifecycle management
2. Same pattern as TimerService - capabilities are external
3. Works on any runtime - if no SpawnService, events are absorbed
4. Factory can be complex - captures any state needed
5. Type-safe replies via `SpawnReplyTo`

---

## Files to Create/Modify

### New Files
- `crates/bloxide-spawn/src/lib.rs` - Spawn service crate
- `crates/bloxide-spawn/src/factory.rs` - `SpawnFactory` trait
- `crates/bloxide-spawn/src/service.rs` - Generic spawn service logic
- `runtimes/bloxide-tokio/src/spawn_service.rs` - Tokio implementation

### Modified Files
- `crates/bloxide-core/src/capability.rs` - Add `SpawnCap` trait (optional, or keep in bloxide-spawn)
- `crates/bloxide-supervisor/src/control.rs` - Add `supervisor_notify` to `SpawnChild` (Option A) or keep as-is (Option B)
- `IMPLEMENTATION_STATUS_WIP.md` - Update with new design

---

## Implementation Phases

### Phase 1: Core Types
1. Create `bloxide-spawn` crate with `SpawnFactory`, `SpawnOutput`
2. Keep supervisor unchanged (already absorbs `SpawnChild`)

### Phase 2: Tokio Implementation
1. Implement `SpawnCap` for `TokioRuntime`
2. Create `spawn_service_listen()` function
3. Create test factory for Worker

### Phase 3: Integration
1. Update `tokio-pool-demo.rs` wiring
2. Test full spawn flow: Pool requests → SpawnService spawns → Supervisor registers

### Phase 4: TestRuntime
1. Implement `SpawnCap` for `TestRuntime`
2. Add integration tests for spawning

### Phase 5: Documentation
1. Update architecture docs
2. Add examples for creating factories
3. Document SpawnService wiring patterns
