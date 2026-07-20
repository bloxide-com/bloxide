# Plan: Capability-Based Child Factory Architecture

**Status**: Approved  
**Created**: 2026-03-12  
**Updated**: 2026-03-12  
**Author**: Code Review  
**Priority**: High  

---

## Problem Statement

The current `ChildType` and `SpawnParams` enums in `bloxide-supervisor/src/control.rs` are hardcoded:

```rust
pub enum ChildType {
    Worker,
    // Add more child types as needed
}

pub enum SpawnParams {
    Worker { task_id: u32 },
    // Add more parameter variants as needed
}
```

This violates a key principle: applications cannot define their own child types without modifying the framework crate. The "Add more child types as needed" comment suggests editing `bloxide-supervisor` directly, which:

1. Forces framework modifications for domain-specific actor types
2. Creates coupling between domain code and framework internals
3. Prevents external crates from defining spawnable actors
4. Violates the layering principle (blox crates depend on framework extensibility)

---

## Solution: Message-Type as Capability

Instead of a hardcoded `ChildType` enum, each **message type** can declare itself spawnable by implementing `SpawnCapability`. The runtime's factory registry uses `TypeId::of::<M>()` as the key - the message type IS the capability.

**Key insight**: A blox like Pool doesn't need to know *what* concrete actor type gets spawned. It only needs:
1. What messages it can send to the spawned peer (`WorkerMsg`)
2. Initial parameters for that peer (`task_id: u32`)
3. A way to communicate with the peer (`ActorRef<WorkerMsg, R>`)

The binary/wiring layer maps the message type to a concrete implementation.

---

## Architecture

### Layer Diagram

```
bloxide-spawn (Layer 2 stdlib)
    ├── SpawnCapability trait
    ├── SpawnedPeer<M, R> struct
    └── SpawnFactoryFor<M, R> trait

pool-messages (Layer 1)
    ├── WorkerMsg enum
    ├── WorkerSpawnParams struct
    └── impl SpawnCapability for WorkerMsg

pool-actions (Layer 2)
    └── request_peer_spawn() generic function
    └── HasWorkers, HasSupervisorControl traits

tokio-pool-demo-impl (Layer 3)
    └── impl SpawnFactoryFor<WorkerMsg, TokioRuntime> for WorkerFactory

bloxide-tokio (Runtime)
    └── SpawnCap::register_spawn_factory::<M, F>()
    └── FACTORIES registry storage

bloxide-supervisor (Layer 2 stdlib)
    └── SupervisorControl::SpawnPeer variant
    └── RegisterChild (unchanged)

tokio-demo binary (Layer 5)
    └── register_spawn_factory::<WorkerMsg, _>(factory)
```

---

## Detailed Design

### 1. `bloxide-spawn` - Core Traits

```rust
// crates/bloxide-spawn/src/capability.rs

use bloxide_core::{capability::BloxRuntime, messaging::ActorRef};
use pool_messages::WorkerCtrl;

/// Marker trait for message types that represent a spawnable peer capability.
///
/// Message crates (Layer 1) implement this on their message types.
/// This is a pure type-association trait - no methods, no behavior.
///
/// # Example
///
/// ```ignore
/// // In pool-messages crate
/// impl SpawnCapability for WorkerMsg {
///     type Params = WorkerSpawnParams;
/// }
/// ```
pub trait SpawnCapability: 'static + Send {
    /// Parameters needed to spawn an actor that handles this message type.
    type Params: Clone + core::fmt::Debug + Send;
}

/// Standard spawn reply for any peer.
///
/// The same structure works for any message type - the blox only needs
/// to know it can send messages of type M to the spawned actor.
#[derive(Debug, Clone)]
pub struct SpawnedPeer<M: Send + 'static, R: BloxRuntime> {
    /// The spawned actor's ID.
    pub child_id: ActorId,
    /// Reference to send domain messages (DoWork, etc.) to the peer.
    pub domain_ref: ActorRef<M, R>,
    /// Reference for peer introduction (AddPeer, RemovePeer).
    pub ctrl_ref: ActorRef<WorkerCtrl<R>, R>,
}
```

```rust
// crates/bloxide-spawn/src/factory.rs

use bloxide_core::messaging::ActorRef;
use bloxide_supervisor::lifecycle::ChildLifecycleEvent;
use crate::{SpawnCapability, SpawnOutput};
use crate::capability::SpawnCap;

/// Factory trait for spawning actors that handle message type M.
///
/// Impl crates (Layer 3) implement this. Binary registers it with the runtime.
///
/// The factory receives typed params (M::Params) and returns SpawnOutput
/// with the lifecycle channel for supervisor registration.
///
/// # Example
///
/// ```ignore
/// // In tokio-pool-demo-impl
/// impl SpawnFactoryFor<WorkerMsg, TokioRuntime> for WorkerFactory {
///     fn spawn(
///         &self,
///         supervisor_notify: ActorRef<ChildLifecycleEvent, TokioRuntime>,
///         params: WorkerSpawnParams,
///         reply_to: Option<TokioRuntime::ErasedReplyTo>,
///     ) -> Option<SpawnOutput<TokioRuntime>> {
///         let task_id = params.task_id;  // Type-safe access
///         // ... create channels, context, spawn task ...
///     }
/// }
/// ```
pub trait SpawnFactoryFor<M, R>: Send + Sync
where
    M: SpawnCapability,
    R: SpawnCap,
{
    fn spawn(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: M::Params,
        reply_to: Option<R::ErasedReplyTo>,
    ) -> Option<SpawnOutput<R>>;
}

/// Type-erased factory for heterogenous storage.
///
/// This blanket impl allows any `SpawnFactoryFor<M, R>` to be stored
/// in the runtime's registry.
pub trait ErasedSpawnFactory<R: SpawnCap>: Send + Sync + 'static {
    fn spawn_erased(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: Box<dyn core::any::Any + Send>,
        reply_to: Option<R::ErasedReplyTo>,
    ) -> Option<SpawnOutput<R>>;
}

impl<M, R, F> ErasedSpawnFactory<R> for F
where
    M: SpawnCapability,
    R: SpawnCap,
    F: SpawnFactoryFor<M, R> + 'static,
{
    fn spawn_erased(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: Box<dyn core::any::Any + Send>,
        reply_to: Option<R::ErasedReplyTo>,
    ) -> Option<SpawnOutput<R>> {
        let typed_params = params.downcast::<M::Params>().ok()?;
        self.spawn(supervisor_notify, *typed_params, reply_to)
    }
}
```

### 2. `pool-messages` - Capability Impl

```rust
// crates/messages/pool-messages/src/lib.rs

use bloxide_spawn::SpawnCapability;

pub enum WorkerMsg {
    DoWork(DoWork),
    WorkDone(WorkDone),
    PeerResult(PeerResult),
}

/// Parameters for spawning a worker that handles WorkerMsg.
#[derive(Debug, Clone)]
pub struct WorkerSpawnParams {
    pub task_id: u32,
}

// Message crate owns the capability binding:
// "WorkerMsg can be used to spawn peers"
impl SpawnCapability for WorkerMsg {
    type Params = WorkerSpawnParams;
}
```

**Note**: The `SpawnCapability` impl does NOT add behavior or runtime types to `WorkerMsg`. It's a pure type association - "if you want to spawn a WorkerMsg handler, here are the params you need."

### 3. `pool-actions` - Request Function

```rust
// crates/actions/pool-actions/src/actions.rs

use bloxide_spawn::{SpawnCapability, SpawnedPeer};
use bloxide_supervisor::SupervisorControl;
use pool_messages::{WorkerMsg, WorkerSpawnParams};

/// Request spawning a peer that can receive WorkerMsg.
///
/// Pool doesn't know or care what concrete actor gets spawned.
/// It just knows it can send WorkerMsg to the result.
pub fn request_peer_spawn<R, C>(ctx: &mut C, task_id: u32)
where
    R: BloxRuntime + DynamicActorSupport + SpawnCap,
    C: HasSelfId + HasSupervisorControl<R>,
{
    let self_id = ctx.self_id();
    
    // Create reply channel for SpawnedPeer<WorkerMsg, R>
    let (reply_ref, reply_rx) = R::channel::<SpawnedPeer<WorkerMsg, R>>(self_id, 4);
    let reply_to = R::erase_reply(reply_ref);
    
    // Send spawn request - message type IS the capability
    let _ = ctx.supervisor_control().try_send(
        self_id,
        SupervisorControl::SpawnPeer {
            capability: core::any::TypeId::of::<WorkerMsg>(),
            params: Box::new(WorkerSpawnParams { task_id }),
            reply_to: Some(reply_to),
        },
    );
    
    ctx.pending_spawn_replies().push((reply_rx, task_id));
}
```

### 4. `bloxide-supervisor` - Control Types

```rust
// crates/bloxide-supervisor/src/control.rs

use core::any::TypeId;

/// Supervisor control-plane events.
#[derive(Clone)]
pub enum SupervisorControl<R: BloxRuntime, Reply = NoSpawnCap> {
    /// Request spawning a peer that can receive messages of type M.
    ///
    /// The spawn service looks up the factory registered for M's TypeId
    /// and calls it with the type-erased params.
    SpawnPeer {
        capability: TypeId,              // TypeId::of::<M>()
        params: Box<dyn Any + Send>,     // M::Params (type-erased)
        reply_to: Option<Reply>,
    },
    /// Register a newly-spawned child with the supervisor.
    RegisterChild(RegisterChild<R>),
    /// Trigger one health-check round.
    HealthCheckTick,
}
```

**Deleted from this file**:
- `ChildType` enum
- `SpawnParams` enum  
- `SpawnedChild` struct (replaced by `SpawnedPeer<M, R>` in `bloxide-spawn`)

### 5. `bloxide-tokio` - Runtime Registry

```rust
// runtimes/bloxide-tokio/src/spawn.rs

use std::collections::HashMap;
use std::sync::{Arc, Mutex, LazyLock};
use bloxide_spawn::{ErasedSpawnFactory, SpawnCapability, SpawnFactoryFor};

type FactoryMap = HashMap<TypeId, Arc<dyn ErasedSpawnFactory<TokioRuntime>>>; 
static FACTORIES: LazyLock<Mutex<FactoryMap>> = 
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl SpawnCap for TokioRuntime {
    // ... erase_reply, send_reply unchanged ...
    
    /// Register a factory for message type M.
    ///
    /// The factory spawns actors that handle messages of type M.
    /// When a blox sends SpawnPeer with TypeId::of::<M>(), this factory runs.
    fn register_spawn_factory<M, F>(factory: F)
    where
        M: SpawnCapability,
        F: SpawnFactoryFor<M, TokioRuntime> + 'static,
    {
        let key = TypeId::of::<M>();
        let mut map = FACTORIES.lock().unwrap();
        if map.contains_key(&key) {
            panic!("register_spawn_factory: factory for {:?} already registered", key);
        }
        map.insert(key, Arc::new(factory));
    }
    
    // spawn_service_listen updated to use SpawnPeer variant
}
```

### 6. `tokio-pool-demo-impl` - Factory Impl

```rust
// crates/impl/tokio-pool-demo-impl/src/lib.rs

use bloxide_spawn::{SpawnFactoryFor, SpawnOutput, SpawnedPeer};
use pool_messages::{WorkerMsg, WorkerSpawnParams};

pub struct WorkerFactory {
    pool_ref: ActorRef<PoolMsg, TokioRuntime>,
}

impl SpawnFactoryFor<WorkerMsg, TokioRuntime> for WorkerFactory {
    fn spawn(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, TokioRuntime>,
        params: WorkerSpawnParams,  // Type-safe, direct field access
        reply_to: Option<TokioRuntime::ErasedReplyTo>,
    ) -> Option<SpawnOutput<TokioRuntime>> {
        let task_id = params.task_id;  // No enum match needed
        
        let child_id = TokioRuntime::alloc_actor_id();
        
        // Create lifecycle channel
        let (lifecycle_ref, lifecycle_rx) = 
            <TokioRuntime as DynamicActorSupport>::channel::<LifecycleCommand>(child_id, 16);
        
        // Create domain channels
        let (domain_ref, domain_rx) = 
            <TokioRuntime as DynamicActorSupport>::channel::<WorkerMsg>(child_id, 16);
        let (ctrl_ref, ctrl_rx) = 
            <TokioRuntime as DynamicActorSupport>::channel::<WorkerCtrl<TokioRuntime>>(child_id, 16);
        
        // Create worker context
        let mut worker_ctx = WorkerCtx::new(child_id, self.pool_ref.clone());
        worker_ctx.task_id = task_id;
        
        let machine = StateMachine::<WorkerSpec<TokioRuntime>>::new(worker_ctx);
        
        // Spawn supervised actor
        let notify_sender = supervisor_notify.sender();
        <TokioRuntime as DynamicActorSupport>::spawn(async move {
            run_supervised_actor(machine, (ctrl_rx, domain_rx), lifecycle_rx, child_id, notify_sender).await;
        });
        
        // Send typed reply
        if let Some(ref reply) = reply_to {
            let _ = TokioRuntime::send_reply(reply, self.pool_ref.id(), SpawnedPeer {
                child_id,
                domain_ref: domain_ref.clone(),
                ctrl_ref: ctrl_ref.clone(),
            });
        }
        
        Some(SpawnOutput {
            child_id,
            lifecycle_ref,
            policy: Some(ChildPolicy::Restart { max: 3 }),
        })
    }
}
```

### 7. Binary - Registration

```rust
// examples/tokio-pool-demo.rs

let worker_factory = WorkerFactory::new(pool_ref.clone());
TokioRuntime::register_spawn_factory::<WorkerMsg, _>(worker_factory);

// Now when Pool requests "spawn something with WorkerMsg capability",
// WorkerFactory runs and creates a Worker actor
```

---

## What Gets Deleted

| File | Deleted | Reason |
|------|---------|--------|
| `bloxide-supervisor/src/control.rs` | `ChildType` enum | Replaced by message TypeId |
| `bloxide-supervisor/src/control.rs` | `SpawnParams` enum | Replaced by `SpawnCapability::Params` |
| `bloxide-supervisor/src/control.rs` | `SpawnedChild` struct | Replaced by `SpawnedPeer<M, R>` |
| `pool-actions/src/traits.rs` | `SpawnedWorker<R>` | Use `SpawnedPeer<WorkerMsg, R>` |
| `bloxide-spawn/src/factory.rs` | `SpawnFactory` (old) | Replaced by `SpawnFactoryFor<M, R>` |

---

## Does This Violate "Messages Are Plain Data"?

**No.** The `SpawnCapability` trait is a pure type-association:

```rust
pub trait SpawnCapability: 'static + Send {
    type Params: Clone + Debug + Send;
}
```

- `WorkerMsg` remains a plain enum with no `ActorRef` or runtime types
- The impl just says "if you want to spawn a WorkerMsg handler, here are the params you need"
- No methods on the message type, no behavior
- Just type metadata for the spawn system

---

## Advantages

| Benefit | Description |
|---------|-------------|
| **Extensibility** | New spawnable actors require zero framework changes |
| **Decoupling** | Pool knows only `WorkerMsg`, not `Worker` actor type |
| **Type safety** | Params are typed per message, no enum matching |
| **Swappable impls** | Tests can register mock `WorkerMsg` handlers |
| **Fewer types** | No `ChildType` enum, no domain-specific spawn types |
| **Layer compliance** | Supervisor is just a blox, no spawn traits in it |

---

## Trade-offs

| Drawback | Mitigation |
|----------|------------|
| **TypeId usage** | Only in spawn service, not hot path |
| **Box<dyn Any>** | Only at spawn boundary, factory gets typed params |
| **Message crate imports bloxide-spawn** | Small dependency for type association |

---

## Files Changed

| File | Change Type | Description |
|------|-------------|-------------|
| `bloxide-spawn/src/capability.rs` | **NEW** | `SpawnCapability` trait, `SpawnedPeer<M, R>` |
| `bloxide-spawn/src/factory.rs` | **MODIFY** | `SpawnFactoryFor<M, R>`, `ErasedSpawnFactory` |
| `bloxide-spawn/src/lib.rs` | **MODIFY** | Re-export new types |
| `bloxide-supervisor/src/control.rs` | **MODIFY** | Delete enums, add `SpawnPeer` variant |
| `pool-messages/src/lib.rs` | **MODIFY** | Add `WorkerSpawnParams`, `impl SpawnCapability` |
| `pool-actions/src/actions.rs` | **MODIFY** | Update `request_peer_spawn` |
| `pool-actions/src/traits.rs` | **MODIFY** | Delete `SpawnedWorker`, update bounds |
| `tokio-pool-demo-impl/src/lib.rs` | **MODIFY** | Implement `SpawnFactoryFor<WorkerMsg, _>` |
| `bloxide-tokio/src/spawn.rs` | **MODIFY** | Registry keyed by `TypeId`, `register_spawn_factory<M, F>` |
| `bloxide-core/src/capability.rs` | **MODIFY** | Remove `ChildType` from `SpawnCap` trait |

---

## Implementation Steps

1. **Add types to `bloxide-spawn`**
   - `SpawnCapability` trait
   - `SpawnedPeer<M, R>` struct
   - `SpawnFactoryFor<M, R>` trait
   - `ErasedSpawnFactory` trait with blanket impl

2. **Update `bloxide-supervisor`**
   - Replace `SpawnChild` variant with `SpawnPeer`
   - Delete `ChildType`, `SpawnParams`, `SpawnedChild`

3. **Update `pool-messages`**
   - Add `WorkerSpawnParams`
   - Add `impl SpawnCapability for WorkerMsg`

4. **Update `pool-actions`**
   - Modify `request_peer_spawn` to use new types
   - Delete `SpawnedWorker`

5. **Update `bloxide-tokio`**
   - Change registry to use `TypeId` keys
   - Update `register_spawn_factory` signature
   - Update spawn service to use `SpawnPeer`

6. **Update `tokio-pool-demo-impl`**
   - Implement `SpawnFactoryFor<WorkerMsg, TokioRuntime>`

7. **Update tests**
   - Update `TestRuntime` registry
   - Update test factories

8. **Run full test suite**

---

## Success Criteria

- [ ] Pool demo spawns workers via `SpawnCapability`
- [ ] `bloxide-supervisor` contains no domain-specific types
- [ ] Adding a new spawnable actor requires zero framework changes
- [ ] All existing tests pass
- [ ] No `ChildType` or `SpawnParams` enums remain

---

## Estimated Effort

| Step | Time | Risk |
|------|------|------|
| Add types to bloxide-spawn | 1 hour | Low |
| Update bloxide-supervisor | 30 min | Low |
| Update pool-messages | 30 min | Low |
| Update pool-actions | 30 min | Low |
| Update bloxide-tokio | 1 hour | Medium |
| Update impl crate | 30 min | Low |
| Update tests | 1 hour | Medium |
| **Total** | **5 hours** | **Medium** |
