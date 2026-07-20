# Implementation Plan: Convention-Based Context Design

## Overview

**Goal**: Simplify context definitions to use naming conventions instead of multiple field annotations. All state goes in behavior objects; context only holds identity + wiring.

**Before**:
```rust
#[derive(BloxCtx)]
pub struct PingCtx<R: BloxRuntime, B: HasCurrentTimer + CountsRounds> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
    pub peer_ref: ActorRef<PingPongMsg, R>,
    #[provides(HasSelfRef<R>)]
    pub self_ref: ActorRef<PingPongMsg, R>,
    #[provides(HasTimerRef<R>)]
    pub timer_ref: ActorRef<TimerCommand, R>,
    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
```

**After**:
```rust
#[derive(BloxCtx)]
pub struct PingCtx<R: BloxRuntime, B: HasCurrentTimer + CountsRounds> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    
    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
```

---

## Phase 1: Update bloxide-macros

### Step 1.1: Design the new macro logic

**Goal**: Define exact inference rules and error messages.

**Acceptance Criteria**:
- Document rules for: `self_id` detection, constructor vs default, trait method matching
- Document error messages for all failure cases
- Document the single remaining annotation: `#[delegates]`

**Files Touched**:
- `crates/bloxide-macros/docs/context-conventions.md` (design doc)

---

### Step 1.2: Implement new inference logic in `#[derive(BloxCtx)]`

**Goal**: Update the macro to implement convention-based inference.

**Acceptance Criteria**:
- `self_id` field auto-detected by name + type
- Non-behavior fields auto-match traits by method name
- Constructor signature generated from type rules
- `#[delegates]` is the only recognized field annotation
- Old annotations (`#[self_id]`, `#[provides]`, `#[ctor]`) emit deprecation warnings but still work

**Files Touched**:
- `crates/bloxide-macros/src/blox_ctx.rs` (main derive logic)
- `crates/bloxide-macros/src/blox_ctx/analyze.rs` (new file: field analysis)
- `crates/bloxide-macros/src/blox_ctx/generate.rs` (new file: code generation)

**Tests**:
- `crates/bloxide-macros/tests/blox_ctx_simple.rs` — basic convention-based context
- `crates/bloxide-macros/tests/blox_ctx_multi_method.rs` — multi-method trait
- `crates/bloxide-macros/tests/blox_ctx_behavior.rs` — behavior delegation
- `crates/bloxide-macros/tests/blox_ctx_errors.rs` — error message tests
- `crates/bloxide-macros/tests/blox_ctx_backward_compat.rs` — old annotations still work

---

### Step 1.3: Add multi-method trait support

**Goal**: Handle traits with getter + setter/mut variants.

**Acceptance Criteria**:
- Field `foo: Vec<T>` generates both `fn foo()` and `fn foo_mut()`
- Field `bar: u32` generates both `fn bar()` and `fn set_bar()`
- Trait definition is inspected to determine which methods to generate

**Files Touched**:
- `crates/bloxide-macros/src/blox_ctx/generate.rs`

**Tests**:
- `crates/bloxide-macros/tests/multi_method_vec.rs`
- `crates/bloxide-macros/tests/multi_method_scalar.rs`

---

### Step 1.4: Add comprehensive error messages

**Goal**: Provide clear, actionable error messages for all failure cases.

**Acceptance Criteria**:
- Every failure mode has a specific, helpful error message
- Error messages suggest fixes
- Errors point to exact field/location

**Files Touched**:
- `crates/bloxide-macros/src/blox_ctx/errors.rs` (new file: error generation)

**Tests**:
- `crates/bloxide-macros/tests/error_field_no_trait.rs`
- `crates/bloxide-macros/tests/error_trait_no_field.rs`
- `crates/bloxide-macros/tests/error_ambiguous_trait.rs`

---

## Phase 2: Update All Blox Context Structs

### Step 2.1: Update CounterCtx

**Goal**: Simplify CounterCtx to use conventions.

**Before**:
```rust
#[derive(BloxCtx)]
pub struct CounterCtx<B: CountsTicks> {
    #[self_id]
    pub self_id: ActorId,
    #[delegates(CountsTicks)]
    pub behavior: B,
}
```

**After**:
```rust
#[derive(BloxCtx)]
pub struct CounterCtx<B: CountsTicks> {
    pub self_id: ActorId,
    
    #[delegates(CountsTicks)]
    pub behavior: B,
}
```

**Acceptance Criteria**:
- No `#[self_id]` annotation needed
- All tests pass

**Files Touched**:
- `crates/bloxes/counter/src/ctx.rs`

**Tests**:
- `cargo test -p counter-blox --features std`

---

### Step 2.2: Update PongCtx

**Goal**: Simplify PongCtx to use conventions.

**Before**:
```rust
#[derive(BloxCtx)]
pub struct PongCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
    pub peer_ref: ActorRef<PingPongMsg, R>,
}
```

**After**:
```rust
#[derive(BloxCtx)]
pub struct PongCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
}
```

**Acceptance Criteria**:
- No annotations needed (accessor trait auto-detected)
- All tests pass

**Files Touched**:
- `crates/bloxes/pong/src/ctx.rs`

**Tests**:
- `cargo test -p pong-blox --features std`

---

### Step 2.3: Update PingCtx

**Goal**: Simplify PingCtx to use conventions.

**Before**:
```rust
#[derive(BloxCtx)]
pub struct PingCtx<R: BloxRuntime, B: HasCurrentTimer + CountsRounds> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
    pub peer_ref: ActorRef<PingPongMsg, R>,
    #[provides(HasSelfRef<R>)]
    pub self_ref: ActorRef<PingPongMsg, R>,
    #[provides(HasTimerRef<R>)]
    pub timer_ref: ActorRef<TimerCommand, R>,
    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
```

**After**:
```rust
#[derive(BloxCtx)]
pub struct PingCtx<R: BloxRuntime, B: HasCurrentTimer + CountsRounds> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    
    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
```

**Acceptance Criteria**:
- All accessor traits auto-detected
- Behavior delegation unchanged
- All tests pass

**Files Touched**:
- `crates/bloxes/ping/src/ctx.rs`

**Tests**:
- `cargo test -p ping-blox --features std`

---

### Step 2.4: Update WorkerCtx (move state to behavior)

**Goal**: Refactor WorkerCtx to have all state in behavior object.

**Before**:
```rust
#[derive(BloxCtx)]
pub struct WorkerCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPoolRef<R>)]
    pub pool_ref: ActorRef<PoolMsg, R>,
    pub task_id: u32,
    pub result: u32,
    pub peers: Vec<ActorRef<WorkerMsg, R>>,
}

impl<R: BloxRuntime> HasWorkerPeers<R> for WorkerCtx<R> { ... }
impl<R: BloxRuntime> HasCurrentTask for WorkerCtx<R> { ... }
```

**After**:
```rust
#[derive(BloxCtx)]
pub struct WorkerCtx<R: BloxRuntime, B: HasWorkerPeers<R> + HasCurrentTask> {
    pub self_id: ActorId,
    pub pool_ref: ActorRef<PoolMsg, R>,
    
    #[delegates(HasWorkerPeers<R>, HasCurrentTask)]
    pub behavior: B,
}
```

**Acceptance Criteria**:
- All state moved to behavior object
- Manual impls removed
- Behavior type parameter added
- All tests pass

**Files Touched**:
- `crates/bloxes/worker/src/ctx.rs`
- `crates/bloxes/worker/src/lib.rs` (update re-exports if needed)
- `crates/bloxes/worker/src/spec.rs` (update WorkerSpec generic bounds)

**Tests**:
- `cargo test -p worker-blox --features std`

---

### Step 2.5: Update PoolCtx (move state to behavior)

**Goal**: Refactor PoolCtx to have all state in behavior object.

**Before**:
```rust
#[derive(BloxCtx)]
pub struct PoolCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[ctor]
    pub self_ref: ActorRef<PoolMsg, R>,
    #[ctor]
    pub worker_factory: WorkerSpawnFn<R>,
    pub worker_refs: Vec<ActorRef<WorkerMsg, R>>,
    pub worker_ctrls: Vec<ActorRef<WorkerCtrl<R>, R>>,
    pub pending: u32,
}

impl<R: BloxRuntime> HasWorkerFactory<R> for PoolCtx<R> { ... }
impl<R: BloxRuntime> HasWorkers<R> for PoolCtx<R> { ... }
```

**After**:
```rust
#[derive(BloxCtx)]
pub struct PoolCtx<R: BloxRuntime, B: HasWorkers<R>> {
    pub self_id: ActorId,
    pub self_ref: ActorRef<PoolMsg, R>,
    pub worker_factory: WorkerSpawnFn<R>,
    
    #[delegates(HasWorkers<R>)]
    pub behavior: B,
}
```

**Acceptance Criteria**:
- All state moved to behavior object
- Manual impls removed
- Behavior type parameter added
- All tests pass

**Files Touched**:
- `crates/bloxes/pool/src/ctx.rs`
- `crates/bloxes/pool/src/lib.rs`
- `crates/bloxes/pool/src/spec.rs`

**Tests**:
- `cargo test -p pool-blox --features std`

---

## Phase 3: Update Impl Crates

### Step 3.1: Verify CounterBehavior type

**Goal**: Verify CounterBehavior in impl crate is correct.

**Files Touched**:
- `crates/impl/counter-demo-impl/src/lib.rs`

**Acceptance Criteria**:
- Existing implementation already correct
- Tests pass

**Tests**:
- `cargo test -p counter-demo-impl`

---

### Step 3.2: Verify PingBehavior type

**Goal**: Verify PingBehavior exists and is correct.

**Files Touched**:
- `crates/impl/embassy-demo-impl/src/lib.rs`

**Acceptance Criteria**:
- Holds round count and current_timer
- Implements CountsRounds and HasCurrentTimer
- Tests pass

**Tests**:
- `cargo test -p embassy-demo-impl`

---

### Step 3.3: Create WorkerBehavior type

**Goal**: Create behavior type for Worker that holds state.

**Files Touched**:
- `crates/impl/tokio-pool-demo-impl/src/lib.rs`

**New Code**:
```rust
extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{capability::BloxRuntime, messaging::ActorRef};
use pool_messages::WorkerCtrl;
use pool_actions::traits::{HasCurrentTask, HasWorkerPeers};
use pool_messages::WorkerMsg;

pub struct WorkerBehavior<R: BloxRuntime> {
    task_id: u32,
    result: u32,
    peers: Vec<ActorRef<WorkerMsg, R>>,
}

impl<R: BloxRuntime> Default for WorkerBehavior<R> {
    fn default() -> Self {
        Self {
            task_id: 0,
            result: 0,
            peers: Vec::new(),
        }
    }
}

impl<R: BloxRuntime> HasCurrentTask for WorkerBehavior<R> {
    fn task_id(&self) -> u32 { self.task_id }
    fn set_task_id(&mut self, id: u32) { self.task_id = id; }
    fn result(&self) -> u32 { self.result }
    fn set_result(&mut self, r: u32) { self.result = r; }
}

impl<R: BloxRuntime> HasWorkerPeers<R> for WorkerBehavior<R> {
    fn peers(&self) -> &[ActorRef<WorkerMsg, R>] { &self.peers }
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>> { &mut self.peers }
}
```

**Acceptance Criteria**:
- Holds task_id, result, peers
- Implements HasCurrentTask and HasWorkerPeers
- Default implementation provided
- Tests pass

**Tests**:
- `cargo test -p tokio-pool-demo-impl`

---

### Step 3.4: Create PoolBehavior type

**Goal**: Create behavior type for Pool that holds state.

**Files Touched**:
- `crates/impl/tokio-pool-demo-impl/src/lib.rs`

**New Code**:
```rust
use bloxide_core::{capability::BloxRuntime, messaging::ActorRef};
use pool_messages::WorkerCtrl;
use pool_actions::traits::HasWorkers;
use pool_messages::{PoolMsg, WorkerMsg};

pub struct PoolBehavior<R: BloxRuntime> {
    worker_refs: Vec<ActorRef<WorkerMsg, R>>,
    worker_ctrls: Vec<ActorRef<WorkerCtrl<R>, R>>,
    pending: u32,
}

impl<R: BloxRuntime> Default for PoolBehavior<R> {
    fn default() -> Self {
        Self {
            worker_refs: Vec::new(),
            worker_ctrls: Vec::new(),
            pending: 0,
        }
    }
}

impl<R: BloxRuntime> HasWorkers<R> for PoolBehavior<R> {
    fn worker_refs(&self) -> &[ActorRef<WorkerMsg, R>] { &self.worker_refs }
    fn worker_refs_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>> { &mut self.worker_refs }
    fn worker_ctrls(&self) -> &[ActorRef<WorkerCtrl<R>, R>] { &self.worker_ctrls }
    fn worker_ctrls_mut(&mut self) -> &mut Vec<ActorRef<WorkerCtrl<R>, R>> { &mut self.worker_ctrls }
    fn pending(&self) -> u32 { self.pending }
    fn set_pending(&mut self, n: u32) { self.pending = n; }
}
```

**Acceptance Criteria**:
- Holds worker_refs, worker_ctrls, pending
- Implements HasWorkers
- Default implementation provided
- Tests pass

**Tests**:
- `cargo test -p tokio-pool-demo-impl`

---

### Step 3.5: Update worker factory to use new types

**Goal**: Update the WorkerSpawnFn to construct WorkerCtx with behavior.

**Files Touched**:
- `crates/impl/tokio-pool-demo-impl/src/lib.rs`

**Changes**: Update the spawn function to create WorkerCtx with WorkerBehavior.

**Acceptance Criteria**:
- WorkerSpawnFn returns correct refs
- WorkerCtx constructed with WorkerBehavior::default()
- Tests pass

**Tests**:
- `cargo run --example tokio-pool-demo`

---

## Phase 4: Update Action Crates

### Step 4.1: Verify accessor trait definitions

**Goal**: Ensure all accessor traits follow naming conventions.

**Files Touched**:
- `crates/actions/ping-pong-actions/src/lib.rs`
- `crates/actions/pool-actions/src/traits.rs`
- `crates/actions/counter-actions/src/lib.rs`

**Verification**:
- `HasPeerRef::peer_ref(&self)` 
- `HasSelfRef::self_ref(&self)` 
- `HasTimerRef::timer_ref(&self)` 
- `HasPoolRef::pool_ref(&self)` 
- `HasWorkerFactory::worker_factory(&self)` 
- `HasWorkers::worker_refs()`, `worker_ctrls()`, `pending()` 
- `HasWorkerPeers::peers()` 
- `HasCurrentTask::task_id()`, `result()` 

**Acceptance Criteria**:
- All accessor trait method names match expected field names
- Documentation updated if needed

**Tests**:
- `cargo test -p ping-pong-actions`
- `cargo test -p pool-actions`
- `cargo test -p counter-actions`

---

### Step 4.2: Add multi-method convention documentation

**Goal**: Document the getter/setter/mut naming conventions.

**Files Touched**:
- `crates/actions/pool-actions/src/traits.rs` (add doc comments)

**New Documentation**:
```rust
/// Accessor for contexts that spawn and track workers.
///
/// Fields expected on the context:
/// - `worker_refs: Vec<ActorRef<WorkerMsg, R>>` generates `worker_refs()` and `worker_refs_mut()`
/// - `worker_ctrls: Vec<ActorRef<WorkerCtrl<R>, R>>` generates `worker_ctrls()` and `worker_ctrls_mut()`
/// - `pending: u32` generates `pending()` and `set_pending()`
pub trait HasWorkers<R: BloxRuntime> { ... }
```

**Acceptance Criteria**:
- All multi-method traits document expected fields
- Convention is clear from doc comments

---

## Phase 5: Update Documentation

### Step 5.1: Update AGENTS.md

**Goal**: Update key invariants and context documentation.

**Files Touched**:
- `AGENTS.md`

**Changes**:
- Update Key Invariant section to reflect new conventions
- Add section on context definition conventions
- Update field annotation reference

**New Section**:
```markdown
## Context Definition Conventions

Context structs use naming conventions instead of field annotations:

| Field | Convention | Generates |
|-------|-----------|-----------|
| `self_id: ActorId` | Always present | `impl HasSelfId` |
| `foo_ref: ActorRef<M, R>` | Matches `HasFooRef::foo_ref()` | Auto accessor impl |
| `foo_factory: fn(...) -> ...` | Matches `HasFooFactory::foo_factory()` | Auto accessor impl |
| `behavior: B` | Must have `#[delegates(Traits)]` | Forwarding impls |

All mutable state belongs in the behavior object, not as direct context fields.
```

**Acceptance Criteria**:
- Documentation matches implementation
- Examples are updated
- Key invariants section is accurate

---

### Step 5.2: Update skills/building-with-bloxide/SKILL.md

**Goal**: Update the building guide with new conventions.

**Files Touched**:
- `skills/building-with-bloxide/SKILL.md`
- `skills/building-with-bloxide/reference.md`

**Changes**:
- Remove `#[self_id]`, `#[provides]`, `#[ctor]` from documentation
- Update context examples
- Add section on behavior types
- Update the five-layer explanation for behavior objects

**Acceptance Criteria**:
- All examples use new conventions
- No references to removed annotations
- Step-by-step workflow updated

---

### Step 5.3: Update spec/architecture/06-actions.md

**Goal**: Update architecture docs on actions and context.

**Files Touched**:
- `spec/architecture/06-actions.md`
- `spec/architecture/12-action-crate-pattern.md`

**Changes**:
- Update accessor trait conventions
- Document behavior vs accessor trait distinction
- Update context examples

**Acceptance Criteria**:
- Architecture docs match implementation
- Patterns are clearly documented

---

### Step 5.4: Update spec/bloxes/*.md

**Goal**: Update blox specs with new context definitions.

**Files Touched**:
- `spec/bloxes/counter.md`
- `spec/bloxes/ping.md`
- `spec/bloxes/pong.md`
- `spec/bloxes/pool.md`
- `spec/bloxes/worker.md`

**Changes**:
- Update Context sections to show new conventions
- Add behavior type definitions where applicable
- Remove annotation references

**Acceptance Criteria**:
- All specs match implementation
- Context examples are accurate

---

### Step 5.5: Update QUICK_REFERENCE.md

**Goal**: Update decision trees and lookup tables.

**Files Touched**:
- `QUICK_REFERENCE.md`

**Changes**:
- Update "How Do I Add Mutable State to a Blox?" table
- Remove annotation reference table
- Add convention reference table

**Acceptance Criteria**:
- Quick reference matches implementation

---

### Step 5.6: Update CHANGELOG.md

**Goal**: Document this breaking change.

**Files Touched**:
- `CHANGELOG.md`

**Changes**:
- Add section for v0.0.2 (or next version) with breaking changes
- List all removed annotations
- Update changelog with breaking changes

**New Section**:
```markdown
## [0.0.2] - YYYY-MM-DD

### Breaking Changes

- **Context definitions now use naming conventions** instead of field annotations
  - `#[self_id]` removed, `self_id` field auto-detected
  - `#[provides(Trait)]` removed, field name must match trait method
  - `#[ctor]` removed, constructor args determined by type
  - `#[delegates(Trait)]` kept, only annotation needed
  
- **All mutable state must go in behavior objects**
  - Context fields for state are no longer allowed
  - Create a behavior type in the impl crate
  - Use `#[delegates]` to forward behavior traits

### Migration Guide

Before:
```rust
#[derive(BloxCtx)]
pub struct MyCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
    pub peer_ref: ActorRef<Msg, R>,
    #[ctor]
    pub config: Config,
    pub count: u32,  // state
}
```

After:
```rust
// In impl crate:
pub struct MyBehavior {
    count: u32,
}
impl Counts for MyBehavior { ... }

// In blox crate:
#[derive(BloxCtx)]
pub struct MyCtx<R: BloxRuntime, B: Counts> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<Msg, R>,
    pub config: Config,  // Config has fn config() method, accessor trait
    
    #[delegates(Counts)]
    pub behavior: B,
}
```
```

---

## Phase 6: Verify All Tests Pass

### Step 6.1: Run full test suite

**Goal**: All tests pass with new implementation.

**Commands**:
```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo fmt --check
```

**Acceptance Criteria**:
- Zero test failures
- Zero clippy warnings
- Zero fmt issues

---

### Step 6.2: Run all examples

**Goal**: All examples work correctly.

**Commands**:
```bash
cargo run --example tokio-minimal-demo
RUST_LOG=trace cargo run --example tokio-demo
RUST_LOG=info cargo run --example tokio-pool-demo
RUST_LOG=trace cargo run --example embassy-demo
```

**Acceptance Criteria**:
- All examples run to completion
- No runtime errors
- Behavior matches expected

---

### Step 6.3: Build documentation

**Goal**: Documentation builds without warnings.

**Commands**:
```bash
cargo doc --all --no-deps
```

**Acceptance Criteria**:
- Zero doc warnings
- All links resolve

---

### Step 6.4: Verify no_std builds

**Goal**: Core crates still build for no_std.

**Commands**:
```bash
cargo build -p bloxide-core --no-default-features
cargo build -p bloxide-timer --no-default-features
cargo build -p bloxide-supervisor --no-default-features
cargo build -p ping-blox --no-default-features
```

**Acceptance Criteria**:
- All core crates build without std
- No std dependencies leak through

---

## Summary Table

| Phase | Steps | Est. Complexity |
|-------|-------|-----------------|
| 1. Update macros | 4 steps | High |
| 2. Update contexts | 5 steps | Medium |
| 3. Update impl crates | 5 steps | Medium |
| 4. Update actions | 2 steps | Low |
| 5. Update docs | 6 steps | Medium |
| 6. Verify | 4 steps | Low |

**Total**: 26 steps across 6 phases
