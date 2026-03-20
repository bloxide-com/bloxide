# Supervisor-Owned Dynamic Spawning

## Problem Statement

Currently, the pool demo uses an **unsupervised** dynamic spawning pattern:
- Pool has a factory that directly spawns workers via `SpawnCap`
- Workers have no lifecycle mailboxes
- Supervisor has no knowledge of these workers
- No restart on failure, no health checks, no lifecycle events

This is inconsistent with the framework's supervision model where **the supervisor sits above all bloxes and owns all lifecycles**.

## Architecture Principle

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

**Key insight**: Pool does NOT spawn workers directly. The supervisor does. Pool just decides "I need a worker" and the supervisor fulfills that via the factory it was configured with.

---

## Trait Consolidation: DynamicActorSupport

### Problem with Current Trait Design

Currently we have two related traits:
- `DynamicChannelCap` - allocate IDs and create channels at runtime
- `SpawnCap` - spawn tasks at runtime

These express the same concern: "Can this runtime support dynamic actors?"

| Runtime | DynamicChannelCap | SpawnCap | Reason |
|---------|------------------|----------|--------|
| TokioRuntime | ✅ | ✅ | Native dynamic spawning |
| EmbassyRuntime | ❌ | ❌ | Compile-time static only |
| TestRuntime | ✅ | ✅ | Test-friendly dynamic |

### Solution: Single Unified Trait (Breaking Change)

**DELETE** `DynamicChannelCap` and `SpawnCap` traits entirely.

**ADD** `DynamicActorSupport` as the single trait for dynamic actor capability:

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

**NO backwards compatibility aliases.** All code using `DynamicChannelCap` or `SpawnCap` must migrate.

### Blox Constraints

```rust
// Bloxes that REQUIRE dynamic spawning (won't compile for Embassy)
impl<R: BloxRuntime + DynamicActorSupport> MachineSpec for PoolSpec<R> { ... }

// Bloxes that work everywhere (normal bound)
impl<R: BloxRuntime> MachineSpec for PongSpec<R> { ... }
impl<R: BloxRuntime> MachineSpec for WorkerSpec<R> { ... }

// Supervisor works everywhere, but dynamic spawning requires the trait
impl<R: BloxRuntime> MachineSpec for SupervisorSpec<R> {
    // Static children, health checks, lifecycle management - all work
    // SpawnChild event handling is feature-gated:
    //   - On Tokio/TestRuntime: handler exists, spawns dynamically
    //   - On Embassy: event is ignored or returns error
}
```

---

## Desired State

1. **Supervisor is the root** - sits above all bloxes
2. **Supervisor handles spawning** - only when `R: DynamicActorSupport`
3. **All bloxes are supervised** - including pool, ping, pong, worker
4. **Dynamic spawning flows through supervisor** - pool requests, supervisor spawns
5. **Compile-time guarantees** - Pool won't compile for Embassy
6. **No legacy traits** - `DynamicChannelCap` and `SpawnCap` are deleted

---

## Design Decision: Supervisor-Mediated Dynamic Spawning

### Flow for Static Children (current)

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

### Flow for Dynamic Children (new)

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
  - Replies to pool with ActorRef (optional)
```

Works on: **Only runtimes with DynamicActorSupport** (Tokio, TestRuntime)

### New Supervisor Event

```rust
// In bloxide-supervisor/src/control.rs
pub enum SupervisorControl<R: BloxRuntime> {
    // Existing
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

---

## Implementation Plan

### Phase 0: Trait Consolidation (Breaking Change)

**DELETE**:
1. `DynamicChannelCap` trait from `bloxide-core/src/capability.rs`
2. `SpawnCap` trait from `bloxide-core/src/capability.rs`
3. All trait aliases and blanket impls

**ADD**:
1. `DynamicActorSupport` trait to `bloxide-core/src/capability.rs`

**MIGRATE** all usages:

| Old | New |
|-----|-----|
| `R: DynamicChannelCap` | `R: DynamicActorSupport` |
| `R: SpawnCap` | `R: DynamicActorSupport` |
| `<R as DynamicChannelCap>::alloc_actor_id()` | `<R as DynamicActorSupport>::alloc_actor_id()` |
| `<R as DynamicChannelCap>::channel()` | `<R as DynamicActorSupport>::channel()` |
| `<R as SpawnCap>::spawn()` | `<R as DynamicActorSupport>::spawn()` |

**Files**:
- `crates/bloxide-core/src/capability.rs` - delete old traits, add new
- `crates/bloxide-core/src/lib.rs` - update exports
- `runtimes/bloxide-tokio/src/lib.rs` - impl `DynamicActorSupport` instead
- `runtimes/bloxide-embassy/src/lib.rs` - no impl needed (remains just `StaticChannelCap`)
- `crates/bloxide-spawn/src/lib.rs` - update trait bounds
- `crates/bloxide-spawn/src/test_impl.rs` - impl `DynamicActorSupport` for TestRuntime
- All files using `DynamicChannelCap` or `SpawnCap` bounds

### Phase 1: Design Documentation

**Create**: `spec/architecture/15-supervisor-owned-spawning.md`

Document:
- Supervisor sits above all bloxes
- `DynamicActorSupport` as the unified capability (replaces two old traits)
- Bloxes bound it to require dynamic spawning
- Supervisor works everywhere, dynamic spawn is feature-gated

**Update**:
- `spec/architecture/00-layered-architecture.md` - replace trait descriptions
- `spec/architecture/10-effects-and-capabilities.md` - delete old traits, add new
- `spec/architecture/11-dynamic-actors.md` - revise factory section, update trait names
- `spec/bloxes/pool.md` - show pool requesting spawn via supervisor
- `spec/bloxes/worker.md` - add lifecycle mailbox

### Phase 2: Supervisor Dynamic Spawn Support

**Files**:
1. `crates/bloxide-supervisor/src/control.rs` - add `SpawnChild` variant
2. `crates/bloxide-supervisor/src/spec.rs` - handle `SpawnChild` event (feature-gated)
3. `crates/bloxide-supervisor/src/ctx.rs` - add factory registry
4. `crates/bloxide-supervisor/src/registry.rs` - add spawn methods

**SupervisorCtx**:
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

// Factory trait is NOT generic over R - it returns Any boxes
// Supervisor unboxes and spawns via DynamicActorSupport
pub trait DynamicChildFactory {
    fn create(&self, params: SpawnParams) -> (Box<dyn Any>, Box<dyn Any>, ActorId);
    // Returns (context, mailboxes, id) as type-erased boxes
}
```

### Phase 3: Pool Blox Changes

**Files**:
1. `crates/bloxes/pool/src/spec.rs` - bound on `DynamicActorSupport`
2. `crates/bloxes/pool/src/ctx.rs` - add supervisor control ref, remove old factory field
3. `crates/actions/pool-actions/src/traits.rs` - update accessor traits
4. `crates/actions/pool-actions/src/actions.rs` - `request_worker_spawn`

**PoolCtx**:
```rust
pub struct PoolCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    
    #[provides(HasSelfRef<R>)]
    pub self_ref: ActorRef<PoolMsg, R>,
    
    // Reference to supervisor's control channel
    #[provides(HasSupervisorControl<R>)]
    pub supervisor_control: ActorRef<SupervisorControl<R>, R>,
    
    // Workers tracking
    #[provides(HasWorkers<R>)]
    pub workers: Vec<WorkerEntry<R>>,
}

// Pool REQUIRES dynamic spawning - won't compile for Embassy
impl<R: BloxRuntime + DynamicActorSupport> MachineSpec for PoolSpec<R> { ... }
```

**Pool action**:
```rust
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

### Phase 4: Worker Blox Changes

**Files**:
1. `crates/bloxes/worker/src/spec.rs` - add lifecycle mailbox to Mailboxes type
2. `crates/bloxes/worker/src/events.rs` - already has Lifecycle variant ✓

**Worker Mailboxes**:
```rust
type Mailboxes<Rt: BloxRuntime> = (
    Rt::Stream<LifecycleCommand>,      // index 0 - lifecycle
    Rt::Stream<PeerCtrl<WorkerMsg, R>>, // index 1 - ctrl  
    Rt::Stream<WorkerMsg>,              // index 2 - domain
);
```

### Phase 5: Tokio Runtime Implementation

**Files**:
1. `runtimes/bloxide-tokio/src/lib.rs` - impl `DynamicActorSupport`
2. `runtimes/bloxide-tokio/src/supervision.rs` - update helpers
3. `runtimes/bloxide-tokio/src/spawn.rs` - DELETE (functionality moved to trait impl)

```rust
// bloxide-tokio/src/lib.rs
impl DynamicActorSupport for TokioRuntime {
    fn alloc_actor_id() -> ActorId {
        bloxide_macros::next_actor_id!()
    }
    
    fn channel<M: Send + 'static>(id: ActorId, capacity: usize) 
        -> (ActorRef<M, Self>, Self::Stream<M>) 
    {
        // existing channel creation code (move from spawn.rs)
    }
    
    fn spawn(future: impl Future<Output = ()> + Send + 'static) {
        tokio::spawn(future);
    }
}
```

### Phase 6: Update Pool Demo

**Files**:
1. `examples/tokio-pool-demo.rs` - complete rewrite for supervised workers
2. `crates/impl/tokio-pool-demo-impl/src/lib.rs` - factory returns construction data

**Wiring**:
```rust
// Pool needs:
// 1. Its own domain channel
// 2. Reference to supervisor's control channel
// 3. Factory function that returns construction data (NOT spawned task)

let ((pool_ref,), pool_mbox) = channels! { PoolMsg(32) };
let pool_id = pool_ref.id();

// Factory returns (ctx, mailboxes, id) - supervisor does the spawning
fn create_worker(task_id: u32, pool_ref: ActorRef<PoolMsg, TokioRuntime>) 
    -> (WorkerCtx<TokioRuntime>, WorkerMailboxes<TokioRuntime>, ActorId)
{
    let worker_id = TokioRuntime::alloc_actor_id();
    let (domain_ref, ctrl_ref, mailboxes) = create_worker_channels(worker_id);
    let ctx = WorkerCtx::new(worker_id, pool_ref, ctrl_ref, task_id);
    (ctx, mailboxes, worker_id)
}

// Wire supervisor with factory
let sup_ctx = SupervisorCtx::new(sup_id, child_group)
    .with_factory(ChildType::Worker, make_factory(pool_ref));

// Wire pool with supervisor control ref
let pool_ctx = PoolCtx::new(pool_id, pool_ref, sup_control_ref.clone());
```

### Phase 7: Tests

**Files**:
1. `crates/bloxide-supervisor/src/tests/dynamic_spawn.rs` - new
2. `crates/bloxes/pool/src/tests.rs` - update for supervised pattern
3. `crates/bloxide-spawn/src/test_impl.rs` - impl `DynamicActorSupport` for TestRuntime

**Tests**:
- Supervisor receives SpawnChild → creates child (Tokio only)
- Embassy supervisor ignores SpawnChild or returns error
- Child fails → supervisor applies policy
- Pool won't compile without DynamicActorSupport bound
- Health checks work for dynamic children
- All usages of old traits are gone

### Phase 8: Documentation Updates

**Files**:
- `QUICK_REFERENCE.md` - decision tree for dynamic spawn
- `AGENTS.md` - add invariant about DynamicActorSupport
- `spec/architecture/09-application.md` - wiring examples
- `spec/architecture/00-layered-architecture.md` - update capability table
- `spec/architecture/10-effects-and-capabilities.md` - delete old traits docs

### Phase 9: Remove Unsupervised Patterns from Demos

**Files**:
- `examples/tokio-pool-demo.rs` - use supervised pattern
- `examples/tokio-minimal-demo.rs` - make supervised or DELETE
- `crates/bloxide-core/src/actor.rs` - keep `run_actor_auto_start` for tests only, add docs
- Delete any other unsupervised demo patterns

**Decision**: `run_actor_auto_start` is for test utilities and truly standalone actors, NOT for demos. All demo actors should be supervised.

### Phase 10: Clean Up Legacy Code

**DELETE**:
- `DynamicChannelCap` trait usage everywhere
- `SpawnCap` trait usage everywhere  
- `runtimes/bloxide-tokio/src/spawn.rs` (moved to trait impl)
- Any macro/helper that used old traits
- Old documentation mentioning old traits

---

## Acceptance Criteria

1. ✅ `DynamicActorSupport` is the SINGLE trait for dynamic actor capability
2. ✅ `DynamicChannelCap` and `SpawnCap` are DELETED (no aliases)
3. ✅ Supervisor works on all runtimes (Tokio, Embassy, TestRuntime)
4. ✅ Supervisor's `SpawnChild` handler only available with `DynamicActorSupport`
5. ✅ Pool bounds on `DynamicActorSupport` - won't compile for Embassy
6. ✅ Pool does NOT call spawn directly, requests via `SupervisorControl`
7. ✅ Workers have lifecycle mailboxes
8. ✅ Health checks work for dynamically spawned children
9. ✅ Restart policies apply to dynamic children
10. ✅ All tests pass
11. ✅ Pool demo shows supervisor-owned spawning
12. ✅ No code uses old trait names

---

## Invariant to Add to AGENTS.md

> **Supervisor owns all spawning** - Blox crates never call `R::spawn()` directly. The supervisor sits above all bloxes and handles lifecycle. Dynamic child creation flows through `SupervisorControl::SpawnChild` requests, which only work on runtimes implementing `DynamicActorSupport`.
>
> Bloxes that require dynamic spawning should bound on `DynamicActorSupport`:
> ```rust
> impl<R: BloxRuntime + DynamicActorSupport> MachineSpec for PoolSpec<R> { ... }
> ```
> This ensures compile-time rejection on Embassy (static wiring only).
>
> `DynamicActorSupport` unifies ID allocation, channel creation, and task spawning - a runtime supports all of these or none. There are no separate traits for these capabilities.

---

## Files Summary

| Phase | Action | Files |
|-------|--------|-------|
| 0 | Delete/Migrate | All files using `DynamicChannelCap` or `SpawnCap` |
| 0 | Modify | `bloxide-core/capability.rs`, `lib.rs`, `bloxide-tokio/lib.rs`, `bloxide-spawn/lib.rs`, `test_impl.rs` |
| 1 | Create | `spec/architecture/15-supervisor-owned-spawning.md` |
| 1 | Modify | `00-layered-architecture.md`, `10-effects-and-capabilities.md`, `11-dynamic-actors.md`, `pool.md`, `worker.md` |
| 2 | Modify | `supervisor/control.rs`, `spec.rs`, `ctx.rs`, `registry.rs` |
| 3 | Modify | `pool/spec.rs`, `ctx.rs`, `pool-actions/traits.rs`, `actions.rs` |
| 4 | Modify | `worker/spec.rs` |
| 5 | Modify | `bloxide-tokio/lib.rs`, `supervision.rs` |
| 5 | Delete | `bloxide-tokio/spawn.rs` |
| 6 | Modify | `examples/tokio-pool-demo.rs`, `tokio-pool-demo-impl/lib.rs` |
| 7 | Create | `bloxide-supervisor/tests/dynamic_spawn.rs` |
| 8 | Modify | `QUICK_REFERENCE.md`, `AGENTS.md`, `09-application.md` |
| 9 | Modify/Delete | `tokio-minimal-demo.rs`, `actor.rs` docs |
| 10 | Delete | All usages of old trait names |

---

## Estimated Effort

- Phase 0: 2 hours (breaking change, migrate all usages)
- Phase 1: 1.5 hours (docs)
- Phase 2: 2 hours (supervisor support)
- Phase 3: 1.5 hours (pool changes)
- Phase 4: 0.5 hours (worker mailbox)
- Phase 5: 1 hour (runtime impl + deletion)
- Phase 6: 2 hours (demo rewrite)
- Phase 7: 2 hours (tests)
- Phase 8: 0.5 hours (docs)
- Phase 9: 0.5 hours (cleanup demos)
- Phase 10: 1 hour (final cleanup)

**Total: ~14.5 hours** (~3 focused sessions)
