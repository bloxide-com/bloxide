# SpawnCap Trait Architecture for no_std/no_alloc

## Problem Statement

The current `SpawnReplyTo<R>` type uses `Arc<dyn Fn...>` for type erasure, which requires `alloc`. This forces `bloxide-supervisor` to depend on `alloc`, violating the principle that standard-library crates should be `no_std` with minimal dependencies.

## Requirements

1. `SpawnCap` trait in `bloxide-spawn` must be `#![no_std]` with **NO** `extern crate alloc`
2. `ErasedReplyTo` must be an associated type of `SpawnCap` — each runtime decides implementation
3. Runtime implementations (TokioRuntime, TestRuntime) can use `alloc`/`std` for their `ErasedReplyTo`
4. `bloxide-supervisor` must NOT use `alloc` — only use the `SpawnCap::ErasedReplyTo` type

## Design

### 1. Trait Definition in `bloxide-spawn`

```rust
// crates/bloxide-spawn/src/lib.rs
#![no_std]
// NO extern crate alloc!

pub mod capability;

// Re-exports
pub use capability::SpawnCap;
pub use factory::SpawnFactory;
pub use output::SpawnOutput;
// ... peer module exports (no alloc needed)
```

```rust
// crates/bloxide-spawn/src/capability.rs
//! Capability trait for dynamic actor spawning service.
//!
//! This is a **Tier 2** capability trait that runtimes implement.

use bloxide_core::{
    capability::{BloxRuntime, DynamicActorSupport, ActorId},
    messaging::ActorRef,
};
use bloxide_supervisor::control::{ChildType, SupervisorControl};
use crate::factory::SpawnFactory;

/// Capability trait for dynamic actor spawning service.
///
/// Runtimes implementing `DynamicActorSupport` should implement this
/// to provide spawn service functionality.
///
/// # Associated Types
///
/// * `ErasedReplyTo` — Type-erased reply channel. The runtime decides the
///   implementation. Tokio/TestRuntime use `Arc<dyn Fn...>`, Embassy doesn't
///   impl `SpawnCap`.
/// * `SpawnServiceHandle` — Type of handle returned by `spawn_service_listen`.
pub trait SpawnCap: BloxRuntime + DynamicActorSupport + Sized {
    /// Type-erased reply channel. Runtime decides implementation.
    ///
    /// - **TokioRuntime**: `ArcErasedSender<TokioRuntime>` (uses `alloc`)
    /// - **TestRuntime**: `ArcErasedSender<TestRuntime>` (uses `alloc`)
    /// - **EmbassyRuntime**: Does not impl `SpawnCap` (static wiring only)
    type ErasedReplyTo: Clone + Send + Sync + 'static;
    
    /// Type of handle returned by `spawn_service_listen`.
    type SpawnServiceHandle;

    /// Create an erased reply from a typed ActorRef.
    ///
    /// The runtime captures the typed sender in its chosen erased form.
    fn erase_reply<M: Send + 'static>(sender: ActorRef<M, Self>) -> Self::ErasedReplyTo;
    
    /// Send a typed reply through the erased channel.
    ///
    /// # Panics
    ///
    /// Panics if the message type doesn't match the original `ActorRef` type.
    /// This indicates a bug in usage — reply types must match the factory's
    /// expected spawn reply type.
    fn send_reply<M: Send + 'static>(
        reply: &Self::ErasedReplyTo,
        from: ActorId,
        msg: M,
    ) -> Result<(), Self::TrySendError>;

    /// Register a factory for a child type.
    fn register_spawn_factory(
        child_type: ChildType,
        factory: impl SpawnFactory<Self> + 'static,
    );

    /// Start the spawn service listening on supervisor control channel.
    fn spawn_service_listen(
        control_rx: Self::Stream<SupervisorControl<Self>>,
        control_ref: ActorRef<SupervisorControl<Self>, Self>,
        supervisor_notify: ActorRef<ChildLifecycleEvent, Self>,
    ) -> (Self::SpawnServiceHandle, Self::Stream<SupervisorControl<Self>>);
}
```

### 2. Type Alias for `SpawnReplyTo`

Instead of a concrete type in `bloxide-supervisor`, `SpawnReplyTo` becomes a simple type alias:

```rust
// crates/bloxide-supervisor/src/control.rs
#![no_std]
// NO extern crate alloc!

use core::fmt;
use bloxide_core::capability::{BloxRuntime, ActorId};

/// Type alias for erased reply channel.
///
/// Defined in terms of the runtime's `SpawnCap::ErasedReplyTo` type.
pub type SpawnReplyTo<R> = <R as SpawnCap>::ErasedReplyTo;

/// Spawned child result.
#[derive(Debug, Clone)]
pub struct SpawnedChild {
    pub child_id: ActorId,
}

/// Register a new child with the supervisor.
pub struct RegisterChild<R: BloxRuntime> {
    pub id: ActorId,
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    pub policy: ChildPolicy,
}

/// Supervisor control-plane events.
#[derive(Clone)]
pub enum SupervisorControl<R: BloxRuntime> {
    /// Request spawning a new child actor.
    SpawnChild {
        child_type: ChildType,
        params: SpawnParams,
        /// Optional reply channel for the parent to receive spawn reply.
        reply_to: Option<SpawnReplyTo<R>>,
    },
    /// Register a new child at runtime with the supervisor.
    RegisterChild(RegisterChild<R>),
    /// Trigger one health-check round.
    HealthCheckTick,
}
```

**Wait — there's a problem!** `SupervisorControl` uses `SpawnReplyTo<R>` which expands to
`<R as SpawnCap>::ErasedReplyTo`. But what if `R` doesn't implement `SpawnCap`?

**Solution:** Use a bound in places that need spawn capability:

```rust
// Two versions of SupervisorControl
pub enum SupervisorControl<R: BloxRuntime> {
    SpawnChild {
        child_type: ChildType,
        params: SpawnParams,
        /// Only Some when R: SpawnCap
        reply_to: Option<R::ErasedReplyTo>,  // <-- No, this doesn't work either
    },
    // ...
}
```

Actually, the cleanest solution is:

1. `SpawnReplyTo` stays as a type alias but requires the `SpawnCap` bound at usage sites
2. `SupervisorControl` has a generic type that defaults to `()` for non-SpawnCap runtimes

**Better approach:** Use an associated type on `BloxRuntime` itself:

```rust
// In bloxide-core
pub trait BloxRuntime {
    /// Type-erased reply channel, if this runtime supports dynamic spawning.
    /// Defaults to `()` for runtimes without spawn capability.
    type ErasedReplyTo: Clone + Send + Sync + 'static = ();
    // ...
}
```

But this pollutes `BloxRuntime` with spawn-specific concerns.

**Final approach:** Keep `SpawnReplyTo` as a type alias in `bloxide-supervisor` but it's only usable when `R: SpawnCap`. The places that use it (SpawnChild control message, pool-actions) already bound on `DynamicActorSupport` and can extend to `SpawnCap`.

Let me revise:

### Revised Design

#### `bloxide-supervisor` (no_std, no alloc)

```rust
// crates/bloxide-supervisor/src/control.rs
#![no_std]

use core::fmt;
use bloxide_core::capability::{BloxRuntime, ActorId};

/// Placeholder type for SpawnReplyTo in generic contexts.
///
/// This is the default type used when `R` doesn't implement `SpawnCap`.
/// The actual type is determined by `SpawnCap::ErasedReplyTo`.
pub struct NoSpawnCap;

/// Spawn reply channel type.
///
/// This resolves to the runtime's erased reply type when `R: SpawnCap`,
/// or `NoSpawnCap` for runtimes without spawn capability.
pub type SpawnReplyTo<R> = <R as SpawnCap>::ErasedReplyTo;
// This requires SpawnCap to be in scope, which can cause issues...

// Alternative: Use a trait
```

Actually, the cleanest solution is:

**`SpawnReplyTo<Runtime>` is defined ONLY in the trait bound context.**

Let me show the full working design:

---

## Complete Design

### File: `crates/bloxide-spawn/src/lib.rs`

```rust
//! Dynamic actor spawning and peer introduction for bloxide.
#![no_std]
// NO extern crate alloc

pub mod capability;
pub mod factory;
pub mod output;
pub mod peer;
pub mod prelude;

#[cfg(test)]
mod tests;

// Re-exports
pub use capability::SpawnCap;
pub use factory::SpawnFactory;
pub use output::SpawnOutput;
// peer module removed - use domain-specific types
```

### File: `crates/bloxide-spawn/src/capability.rs`

```rust
//! Capability trait for spawn service.

use bloxide_core::{
    capability::{BloxRuntime, DynamicActorSupport, ActorId},
    messaging::ActorRef,
};
use bloxide_supervisor::control::{ChildType, SupervisorControl};
use bloxide_supervisor::lifecycle::ChildLifecycleEvent;
use crate::SpawnFactory;

/// Capability trait for dynamic actor spawning.
///
/// Only runtimes supporting dynamic actor creation implement this.
/// Embassy does NOT implement this trait (static wiring only).
pub trait SpawnCap: BloxRuntime + DynamicActorSupport + Sized {
    /// Type-erased reply channel for spawn results.
    ///
    /// Each runtime chooses its implementation:
    /// - TokioRuntime: `ArcErasedSender` (uses `alloc`)
    /// - TestRuntime: `ArcErasedSender` (uses `alloc`)
    ///
    /// Must be `Clone + Send + Sync + 'static`.
    type ErasedReplyTo: Clone + Send + Sync + 'static;
    
    /// Handle type for the spawn service task.
    type SpawnServiceHandle;

    /// Erase a typed ActorRef into the runtime's erased reply type.
    fn erase_reply<M: Send + 'static>(sender: ActorRef<M, Self>) -> Self::ErasedReplyTo;
    
    /// Send a typed message through an erased reply channel.
    ///
    /// # Panics
    ///
    /// Panics on type mismatch. This indicates a bug in the factory's
    /// reply logic.
    fn send_reply<M: Send + 'static>(
        reply: &Self::ErasedReplyTo,
        from: ActorId,
        msg: M,
    ) -> Result<(), Self::TrySendError>;

    /// Register a spawn factory for a child type.
    fn register_spawn_factory(
        child_type: ChildType,
        factory: impl SpawnFactory<Self> + 'static,
    );

    /// Start the spawn service.
    fn spawn_service_listen(
        control_rx: Self::Stream<SupervisorControl<Self>>,
        control_ref: ActorRef<SupervisorControl<Self>, Self>,
        supervisor_notify: ActorRef<ChildLifecycleEvent, Self>,
    ) -> (Self::SpawnServiceHandle, Self::Stream<SupervisorControl<Self>>);
}
```

### File: `crates/bloxide-supervisor/src/control.rs`

```rust
//! Supervisor control messages.
#![no_std]
// NO extern crate alloc

use core::fmt;
use bloxide_core::capability::{BloxRuntime, ActorId, ActorRef};
use crate::lifecycle::LifecycleCommand;
use crate::registry::ChildPolicy;

/// Marker type indicating no spawn capability.
///
/// Used in `SupervisorControl` when `R` doesn't implement `SpawnCap`.
#[derive(Clone)]
pub struct NoSpawnCap;

impl fmt::Debug for NoSpawnCap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NoSpawnCap")
    }
}

/// Type alias for erased spawn reply.
///
/// Requires `R: SpawnCap` to resolve. Otherwise, use `NoSpawnCap`.
///
/// In practice, this is only used when `R: SpawnCap` (Tokio/TestRuntime).
/// Embassy uses `NoSpawnCap` and never sends `SpawnChild` messages.
pub type SpawnReplyTo<R> = spawn_reply_to::<R>();

// Helper trait to resolve the type
mod private {
    use super::*;
    
    pub trait SpawnReplyType {
        type Reply: Clone + Send + Sync + 'static;
    }
    
    impl SpawnReplyType for () {
        type Reply = NoSpawnCap;
    }
}

fn spawn_reply_to<R>() -> R::ErasedReplyTo
where R: SpawnCap
{
    // This doesn't actually work... need a different approach
}
```

Hmm, type aliases can't have bounds that work this way. Let me try a different approach using an associated type with a default:

### Alternative: Associated Type with Default (Best Solution)

```rust
// crates/bloxide-supervisor/src/control.rs
#![no_std]

/// Trait to resolve erased reply type.
///
/// Implemented for all BloxRuntimes. SpawnCap implementations provide
/// their own `ErasedReplyTo` type. Non-SpawnCap runtimes get `NoSpawnCap`.
pub trait HasErasedReply {
    type ErasedReplyTo: Clone + Send + Sync + 'static;
}

/// Marker type for runtimes without spawn capability.
#[derive(Clone, Copy, Debug)]
pub struct NoSpawnCap;

/// Non-SpawnCap runtimes get NoSpawnCap as their reply type.
impl<R: BloxRuntime> HasErasedReply for R {
    default type ErasedReplyTo = NoSpawnCap;
}

/// SpawnCap runtimes provide their own ErasedReplyTo.
impl<R: SpawnCap> HasErasedReply for R {
    type ErasedReplyTo = R::ErasedReplyTo;
}
```

Wait, this requires specialization which isn't stable. Let me think...

### Practical Solution: Two Message Types

The simplest working solution:

1. `bloxide-supervisor` defines `SpawnReplyTo<R>` generic over any type
2. Runtime implementations provide their erased type
3. Bounds are enforced at usage sites

```rust
// crates/bloxide-supervisor/src/control.rs
#![no_std]

/// Supervisor control messages.
///
/// Generic over the erased reply type so non-SpawnCap runtimes can use
/// a zero-sized type.
#[derive(Clone)]
pub enum SupervisorControl<R: BloxRuntime, Reply = NoSpawnCap> {
    SpawnChild {
        child_type: ChildType,
        params: SpawnParams,
        reply_to: Option<Reply>,
    },
    RegisterChild(RegisterChild<R>),
    HealthCheckTick,
}

/// Marker type for non-SpawnCap runtimes.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoSpawnCap;

/// Alias for SpawnCap-using SupervisorControl.
pub type SpawnCapControl<R> = SupervisorControl<R, <R as SpawnCap>::ErasedReplyTo>;
```

This allows:
- `SupervisorControl<TokioRuntime, ArcErasedSender>` — spawns work
- `SupervisorControl<EmbassyRuntime, NoSpawnCap>` — spawns never used

But this bifurcates `SupervisorControl` types throughout the codebase...

---

## Final Recommendation: Keep It Simple

After analysis, the cleanest approach is:

### `bloxide-supervisor` uses a placeholder type

```rust
// crates/bloxide-supervisor/src/control.rs
#![no_std]

/// Phantom type for spawn reply when R: !SpawnCap.
#[derive(Clone, Copy, Debug)]
pub struct NoSpawnCap;

/// SpawnReplyTo resolves to the runtime's erased reply when R: SpawnCap,
/// or NoSpawnCap for non-spawn-capable runtimes.
///
/// Note: This type alias only works when SpawnCap is in scope and R: SpawnCap.
/// Otherwise use `NoSpawnCap` directly.
pub type SpawnReplyTo<R> = <R as SpawnCap>::ErasedReplyTo;
```

The usage sites handle the bound:

```rust
// pool-actions: already bounds on DynamicActorSupport
pub fn request_worker_spawn<R, C>(ctx: &mut C, task_id: u32)
where
    R: BloxRuntime + SpawnCap,  // <-- Add SpawnCap bound
    C: HasSupervisorControl<R>,
{
    let reply_to = R::erase_reply(reply_ref);
    // ...
}
```

### Runtime Implementation

```rust
// runtimes/bloxide-tokio/src/lib.rs
#![no_std]
extern crate alloc;  // <-- Runtime CAN use alloc

// runtimes/bloxide-tokio/src/erased_reply.rs
use alloc::sync::Arc;
use alloc::boxed::Box;
use core::any::{Any, TypeId};
use bloxide_core::{BloxRuntime, ActorId, ActorRef};

/// Type-erased sender using Arc<dyn Fn>.
pub struct ArcErasedSender<R: BloxRuntime> {
    id: ActorId,
    send_fn: Arc<dyn Fn(ActorId, Box<dyn Any + Send>) -> Result<(), R::TrySendError> + Send + Sync>,
    type_id: TypeId,
}

impl<R: BloxRuntime> Clone for ArcErasedSender<R> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            send_fn: Arc::clone(&self.send_fn),
            type_id: self.type_id,
        }
    }
}

impl SpawnCap for TokioRuntime {
    type ErasedReplyTo = ArcErasedSender<Self>;
    type SpawnServiceHandle = tokio::task::JoinHandle<()>;
    
    fn erase_reply<M: Send + 'static>(sender: ActorRef<M, Self>) -> Self::ErasedReplyTo {
        let id = sender.id();
        let send_fn = Arc::new(move |from: ActorId, msg: Box<dyn Any + Send>| {
            match msg.downcast::<M>() {
                Ok(typed) => sender.try_send(from, *typed),
                Err(_) => panic!("ArcErasedSender: type mismatch"),
            }
        });
        ArcErasedSender {
            id,
            send_fn,
            type_id: TypeId::of::<M>(),
        }
    }
    
    fn send_reply<M: Send + 'static>(
        reply: &Self::ErasedReplyTo,
        from: ActorId,
        msg: M,
    ) -> Result<(), Self::TrySendError> {
        debug_assert_eq!(reply.type_id, TypeId::of::<M>());
        (reply.send_fn)(from, Box::new(msg))
    }
    
    // ... register_spawn_factory, spawn_service_listen implementations
}
```

### Factory Trait Uses the Associated Type

```rust
// crates/bloxide-spawn/src/factory.rs
#![no_std]

pub trait SpawnFactory<R: SpawnCap>: Send + Sync {
    fn spawn(
        &self,
        supervisor_id: ActorId,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: SpawnParams,
        reply_to: Option<R::ErasedReplyTo>,  // <-- Uses associated type
    ) -> Option<SpawnOutput<R>>;
}
```

## Summary of Changes

| Crate | Current | After |
|-------|---------|-------|
| `bloxide-spawn` | `#![no_std]` + `extern crate alloc` | `#![no_std]` only |
| `bloxide-supervisor` | `#![no_std]` + `extern crate alloc` | `#![no_std]` only |
| `runtimes/bloxide-tokio` | alloc in spawn.rs | alloc in erased_reply.rs |
| `SpawnReplyTo<R>` | Concrete struct in supervisor | Type alias to `SpawnCap::ErasedReplyTo` |

## Benefits

1. **Clean separation:** Core stdlib crates (`spawn`, `supervisor`) are truly `no_std` with no alloc
2. **Runtime flexibility:** Each runtime chooses its erased reply implementation
3. **Embassy compatibility:** Doesn't impl `SpawnCap`, so has no erased reply type
4. **Type safety:** Bounds enforced at usage sites, no runtime type errors (panics only on bugs)

## Migration Path

1. Add `ErasedReplyTo` associated type to `SpawnCap`
2. Add `erase_reply` and `send_reply` methods to `SpawnCap`
3. Move `ArcErasedSender` from supervisor to tokio runtime
4. Change `SpawnReplyTo<R>` from struct to type alias
5. Update factory trait to use `R::ErasedReplyTo`
6. Update usage sites in pool-actions, tokio-pool-demo-impl
