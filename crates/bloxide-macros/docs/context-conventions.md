// Copyright 2025 Bloxide, all rights reserved
# Convention-Based Context Design

This document specifies the inference rules, error messages, and conventions
for `#[derive(BloxCtx)]` macro behavior.

## Overview

Context structs use naming conventions instead of explicit field annotations.
Only one annotation (`#[delegates]`) is required for behavior delegation fields.

## Field Role Inference Rules

### Rule 1: `self_id` Detection

**Pattern**: Field named `self_id` with type `ActorId`

```rust
pub self_id: ActorId,  // Auto-detects as SelfId role
```

**Generated Code**:
- `impl HasSelfId for Struct` with `fn self_id(&self) -> ActorId`

**Type Matching**:
- Exact match: `ActorId`
- Qualified match: `bloxide_core::messaging::ActorId` or `::bloxide_core::ActorId`

**Error Conditions**:
- Multiple `self_id` fields: compile error "BloxCtx: multiple `self_id` fields found; only one is allowed"
- Field named `self_id` but wrong type: falls through to other rules (not treated as SelfId)

---

### Rule 2: ActorRef Accessor Detection

**Pattern**: Field name ending with `_ref` and type `ActorRef<M, R>`

```rust
pub peer_ref: ActorRef<PingPongMsg, R>,     // → HasPeerRef<R>
pub self_ref: ActorRef<PingPongMsg, R>,     // → HasSelfRef<R>
pub timer_ref: ActorRef<TimerCommand, R>,   // → HasTimerRef<R>
pub pool_ref: ActorRef<PoolMsg, R>,         // → HasPoolRef<R>
```

**Inference Logic**:
1. Check if field name ends with `_ref`
2. Check if type is `ActorRef<M, R>` or `path::to::ActorRef<M, R>`
3. Convert field name to trait name: `peer_ref` → `HasPeerRef`
4. Extract runtime generic `R` from `ActorRef<M, R>` (last type argument)
5. Generate trait path: `HasPeerRef<R>`

**Name Conversion**: `snake_case_ref` → `HasSnakeCaseRef<R>`
- Split by underscore
- Capitalize each word
- Drop the final `Ref` from field name, add `Has` prefix and `Ref` suffix
- Result: `peer_ref` → `HasPeerRef`, `worker_ctrl_ref` → `HasWorkerCtrlRef`

**Generated Code**:
```rust
impl<R: BloxRuntime> HasPeerRef<R> for Struct<R> {
    fn peer_ref(&self) -> &ActorRef<PingPongMsg, R> {
        &self.peer_ref
    }
}
```

---

### Rule 3: Factory Accessor Detection

**Pattern**: Field name ending with `_factory` and type `fn(...) -> ...`

```rust
pub worker_factory: WorkerSpawnFn<R>,  // → HasWorkerFactory<R>
pub task_factory: fn(Config) -> Task,  // → HasTaskFactory
```

**Inference Logic**:
1. Check if field name ends with `_factory`
2. Check if type is a function pointer (`fn`, `Fn`, `FnMut`, `FnOnce`) or type alias thereof
3. Convert field name to trait name: `worker_factory` → `HasWorkerFactory`
4. If type contains runtime generic, include it in trait path

**Generated Code**:
```rust
impl<R: BloxRuntime> HasWorkerFactory<R> for Struct<R> {
    fn worker_factory(&self) -> WorkerSpawnFn<R> {
        self.worker_factory
    }
}
```

**Note**: Factory accessors return by value (copy) rather than by reference,
since function pointers are `Copy`.

---

### Rule 4: Behavior Delegation

**Pattern**: Field with `#[delegates(Trait1, Trait2, ...)]` annotation

```rust
#[delegates(CountsRounds, HasCurrentTimer)]
pub behavior: B,
```

**Required Annotation**: This is the ONLY required field annotation.

**Generated Code**: For each trait `T` in the list:
```rust
__delegate_T!(
    struct_name: StructName,
    field: behavior,
    field_type: B,
    impl_generics: { <R: BloxRuntime, B: T> },
    ty_generics: { <R, B> },
    where_clause: { where B: T }
);
```

The `__delegate_T!` macro is provided by the `delegatable!` macro in the
action crate. It generates:
```rust
impl<R: BloxRuntime, B: CountsRounds> CountsRounds for Struct<R, B> {
    fn round(&self) -> B::Round { self.behavior.round() }
    fn set_round(&mut self, r: B::Round) { self.behavior.set_round(r); }
}
```

---

### Rule 5: Constructor Parameter Detection

**Pattern**: ActorRef fields not matching `_ref` suffix, or fields with `#[ctor]`

```rust
// Acting as constructor param (passed to new()):
pub config: Config,                 // Not ActorRef, not _ref/_factory → state (deprecated)
pub channel: ActorRef<Msg, R>,      // Not _ref suffix → constructor param
```

**Inference Logic**:
- If field is `ActorRef` but doesn't end in `_ref`: treat as constructor parameter
- If field has `#[ctor]` annotation: treat as constructor parameter
- Otherwise: treat as state field (deprecated path)

**Constructor Generation**:

All non-state fields become constructor parameters. State fields are
zero-initialized via `Default::default()`.

```rust
// Generated constructor:
impl<R: BloxRuntime, B: CountsRounds> PingCtx<R, B> {
    pub fn new(
        self_id: ActorId,
        peer_ref: ActorRef<PingPongMsg, R>,
        self_ref: ActorRef<PingPongMsg, R>,
        timer_ref: ActorRef<TimerCommand, R>,
        behavior: B,
    ) -> Self {
        Self {
            self_id,
            peer_ref,
            self_ref,
            timer_ref,
            behavior,
        }
    }
}
```

---

### Rule 6: State Fields (Deprecated)

**Pattern**: Fields that don't match any of the above patterns

```rust
pub task_id: u32,    // State field (deprecated)
pub result: u32,     // State field (deprecated)
pub peers: Vec<T>,   // State field (deprecated)
```

**Behavior**: These fields are zero-initialized via `Default::default()`.
No trait implementations are generated.

**Deprecation**: This pattern is deprecated. All mutable state should be
moved to behavior objects. See Migration Guide below.

---

## Constructor Signature Generation

The constructor `new()` signature is generated from field roles:

| Role | Constructor Parameter? | Initialization |
|------|----------------------|----------------|
| SelfId | Yes | Passed in |
| Ctor | Yes | Passed in |
| Accessor | Yes | Passed in |
| Delegates | Yes | Passed in |
| State | No | `Default::default()` |

**Signature Order**: Fields appear in constructor in declaration order.

**Example**:
```rust
#[derive(BloxCtx)]
pub struct PoolCtx<R: BloxRuntime> {
    pub self_id: ActorId,                        // SelfId → param
    pub self_ref: ActorRef<PoolMsg, R>,          // Ctor → param (ActorRef not _ref)
    pub worker_factory: WorkerSpawnFn<R>,        // Accessor → param
    pub pending: u32,                            // State → default
}

// Generated:
impl<R: BloxRuntime> PoolCtx<R> {
    pub fn new(
        self_id: ActorId,
        self_ref: ActorRef<PoolMsg, R>,
        worker_factory: WorkerSpawnFn<R>,
    ) -> Self {
        Self {
            self_id,
            self_ref,
            worker_factory,
            pending: Default::default(),  // zero
        }
    }
}
```

---

## Multi-Method Trait Support

For traits with multiple methods (getter + setter, or getter + mut getter),
the trait definition determines the generated implementation:

### Vec Fields
```rust
// Field:
pub peers: Vec<ActorRef<WorkerMsg, R>>,

// Trait:
pub trait HasPeers<M, R: BloxRuntime> {
    fn peers(&self) -> &[ActorRef<M, R>];
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<M, R>>;
}

// Generated impl (when field matches trait method name):
impl<M, R: BloxRuntime> HasPeers<M, R> for Struct<M, R> {
    fn peers(&self) -> &[ActorRef<M, R>] { &self.peers }
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<M, R>> { &mut self.peers }
}
```

### Scalar Fields with Setter
```rust
// Field:
pub pending: u32,

// Trait:
pub trait HasPending {
    fn pending(&self) -> u32;
    fn set_pending(&mut self, n: u32);
}

// Generated impl:
impl HasPending for Struct {
    fn pending(&self) -> u32 { self.pending }
    fn set_pending(&mut self, n: u32) { self.pending = n; }
}
```

**Inference Approach**: The macro inspects the trait definition to determine
required methods and generates implementations based on field type patterns.

---

## Annotations Reference

### Required Annotation

| Annotation | Purpose | Usage |
|------------|---------|-------|
| `#[delegates(T1, T2, ...)]` | Mark field as behavior delegator | Behavior fields only |

### Deprecated Annotations (Backward Compatibility)

| Annotation | Replacement | Notes |
|-----------|-------------|-------|
| `#[self_id]` | Naming convention `self_id: ActorId` | Still works, emits deprecation warning |
| `#[provides(Trait)]` | Naming convention `foo_ref: ActorRef<M, R>` | Still works, emits deprecation warning |
| `#[ctor]` | Naming convention or type-based inference | Still works, emits deprecation warning |

---

## Error Messages

### Error: Multiple `self_id` Fields

```
error: BloxCtx: multiple `self_id` fields found; only one is allowed
 --> src/ctx.rs:10:5
  |
10|     pub self_id: ActorId,
  |     ^^^^^^^^^^^^^^^^^^^
```

**Fix**: Remove duplicate `self_id` field.

---

### Error: Duplicate Annotation on Field

```
error: BloxCtx: a field may only have one BloxCtx annotation
 --> src/ctx.rs:8:5
  |
8 |     #[self_id]
9 |     #[provides(HasFooRef<R>)]
10|     pub foo: ActorRef<Msg, R>,
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^
```

**Fix**: Remove one of the annotations.

---

### Error: Empty Delegates List

```
error: BloxCtx: #[delegates(...)] requires at least one trait
 --> src/ctx.rs:5:5
  |
5 |     #[delegates()]
6 |     pub behavior: B,
  |     ^^^^^^^^^^^^^^^
```

**Fix**: Add trait names: `#[delegates(CountsRounds)]`

---

### Error: Invalid Annotation Format

```
error: BloxCtx: expected parenthesized argument, e.g., #[provides(HasPeerRef<R>)]
 --> src/ctx.rs:5:5
  |
5 |     #[provides]
6 |     pub peer_ref: ActorRef<Msg, R>,
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**Fix**: Add trait in parentheses: `#[provides(HasPeerRef<R>)]`

---

### Error: Struct Must Have Named Fields

```
error: #[derive(BloxCtx)] requires named fields
 --> src/ctx.rs:3:10
  |
3 | pub struct MyCtx(u32, u32);
  |          ^^^^^^
```

**Fix**: Use named fields:
```rust
pub struct MyCtx {
    pub foo: u32,
    pub bar: u32,
}
```

---

### Error: Only Structs Supported

```
error: #[derive(BloxCtx)] only supports structs
 --> src/ctx.rs:3:10
  |
3 | pub enum MyEnum { ... }
  |          ^^^^^^
```

**Fix**: Use a struct instead.

---

### Warning: Deprecation Notice

```
warning: use of deprecated annotation
 --> src/ctx.rs:5:5
  |
5 |     #[self_id]
6 |     pub self_id: ActorId,
  |     ^^^^^^^^^^^^^^^^^^^^
  |
  = note: BloxCtx: using deprecated annotations. Use convention-based fields instead:
          `self_id: ActorId` is auto-detected, `_ref` fields auto-generate accessor traits.
          Only `#[delegates(...)]` is required for behavior fields.
```

**Fix**: Remove the annotation - the field is auto-detected by convention.

---

## Migration Guide

### From State Fields to Behavior Objects

**Before** (deprecated pattern):
```rust
#[derive(BloxCtx)]
pub struct WorkerCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    pub task_id: u32,      // state
    pub result: u32,       // state
    pub peers: Vec<T>,     // state
}

impl<R: BloxRuntime> HasCurrentTask for WorkerCtx<R> { ... }
impl<R: BloxRuntime> HasPeers for WorkerCtx<R> { ... }
```

**After** (recommended pattern):

1. Create behavior type in impl crate:
```rust
// impl/tokio-pool-demo-impl/src/lib.rs
pub struct WorkerBehavior<R: BloxRuntime> {
    task_id: u32,
    result: u32,
    peers: Vec<ActorRef<WorkerMsg, R>>,
}

impl<R: BloxRuntime> HasCurrentTask for WorkerBehavior<R> { ... }
impl<R: BloxRuntime> HasPeers<WorkerMsg, R> for WorkerBehavior<R> { ... }
```

2. Update context to delegate:
```rust
// bloxes/worker/src/ctx.rs
#[derive(BloxCtx)]
pub struct WorkerCtx<R: BloxRuntime, B: HasCurrentTask + HasPeers<WorkerMsg, R>> {
    pub self_id: ActorId,
    pub pool_ref: ActorRef<PoolMsg, R>,
    
    #[delegates(HasCurrentTask, HasPeers<WorkerMsg, R>)]
    pub behavior: B,
}
```

### From Old Annotations to Conventions

| Before | After |
|--------|-------|
| `#[self_id] pub self_id: ActorId` | `pub self_id: ActorId` |
| `#[provides(HasPeerRef<R>)] pub peer_ref: ActorRef<M, R>` | `pub peer_ref: ActorRef<M, R>` |
| `#[ctor] pub config: Config` | `pub config: Config` (needs `fn config()` accessor trait) |
| `#[ctor] pub factory: FactoryFn` | `pub factory: FactoryFn` (auto-detected as `_factory` suffix) |

---

## Summary Table

| Field Pattern | Inferred Role | Annotation Needed? |
|---------------|---------------|-------------------|
| `self_id: ActorId` | SelfId | No (auto-detected) |
| `*_ref: ActorRef<M, R>` | Accessor | No (auto-detected) |
| `*_factory: fn(...)` | Accessor | No (auto-detected) |
| `behavior: B` with traits | Delegates | **Yes**: `#[delegates(T1, T2)]` |
| Other ActorRef fields | Ctor param | No |
| Other types | State (deprecated) | No |

**Only `#[delegates]` is required** for marking behavior delegation fields.
All other roles are inferred from naming conventions and type patterns.
