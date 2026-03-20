# Implementation Plan: Consistency & Agent-Friendliness Fixes

**Created**: 2026-03-11
**Auditors**: Agent team (Singer, Mendel, Ramanujan, Faraday, Linnaeus)
**Implementers**: TBD

This plan addresses all issues found during the consistency audit. Each item has:
- Exact file path
- Specific line numbers or ranges
- Exact text to add/modify
- Verification steps

---

## Phase 1: Critical Documentation Fixes

### 1.1 Document BloxRuntime Trait

**File**: `crates/bloxide-core/src/capability.rs`
**Lines**: 10-41 (trait definition)

**Current state**: Trait has no documentation.

**Action**: Replace the trait definition with:

```rust
/// Base trait for runtime-specific message sending and receiving.
///
/// This is the **only** trait that blox crates are generic over (`<R: BloxRuntime>`).
/// It abstracts the runtime-specific channel implementations while keeping
/// blox code runtime-agnostic.
///
/// # Associated Types
///
/// * `Sender<M>` — Send-side of a typed channel. Must be cheaply cloneable
///   (typically `Arc`-based) since `ActorRef<M>` clones it.
/// * `Receiver<M>` — Receive-side of a typed channel.
/// * `Stream<M>` — A fused stream that yields messages of type `M`.
/// * `SendError` — Error returned by `send_via` when the operation fails
///   (e.g., channel closed, capacity exhausted).
/// * `TrySendError` — Error returned specifically by `try_send_via` when
///   the channel buffer is full (non-blocking send failed).
///
/// # When to Use Which Send Method
///
/// * `send_via` — Use when you want to wait for capacity (async).
/// * `try_send_via` — Use when you need non-blocking behavior; check the
///   result for `Ok(())` vs `Err(TrySendError)`.
///
/// # Converting Receivers to Streams
///
/// Use `to_stream(receiver)` to convert a `Receiver<M>` into a `Stream<M>`.
/// This is how the run loop receives messages via `futures::StreamExt::next`.
pub trait BloxRuntime: Sized {
    type Sender<M>: Clone + Send + Sync + Unpin;
    type Receiver<M>: Send + Sync + Unpin;
    type Stream<M>: Send + Sync + Unpin + futures_core::Stream<Item = M>;
    type SendError: core::fmt::Debug;
    type TrySendError: core::fmt::Debug;

    fn to_stream<M>(rx: Self::Receiver<M>) -> Self::Stream<M>;
    fn send_via<M, R>(sender: &Self::Sender<M>, msg: M) -> impl Future<Output = Result<(), Self::SendError>> + Send;
    fn try_send_via<M>(sender: &Self::Sender<M>, msg: M) -> Result<(), Self::TrySendError>;
}
```

**Verification**: Run `cargo doc --package bloxide-core --open` and verify the trait documentation renders.

---

### 1.2 Update AGENTS.md "Where to Find Things" Table

**File**: `AGENTS.md`
**Lines**: 76-96 (the "Where to Find Things" table)

**Action**: Add the following rows to the table (insert after the row ending with `12-action-crate-pattern.md`):

```markdown
| How do timers and capabilities work? | `spec/architecture/10-effects-and-capabilities.md` |
| How do dynamic actors and factory injection work? | `spec/architecture/11-dynamic-actors.md` |
| How do I test timers without an executor? | `crates/bloxide-timer/src/test_utils.rs` (`VirtualClock`) |
```

**Verification**: Confirm the table has rows for architecture docs 00-12, plus VirtualClock.

---

### 1.3 Update AGENTS.md Repository Layout

**File**: `AGENTS.md`
**Lines**: 15-58 (repository layout tree)

**Action**: Add the following entries to the tree structure.

Add under `messages/`:
```text
      counter-messages/       ← CounterMsg shared by counter blox and minimal wiring demo
```

Add under `actions/`:
```text
      counter-actions/        ← CountsTicks behavior trait and increment_count action
```

Add under `impl/`:
```text
      counter-demo-impl/      ← impl crate: CounterBehavior for tokio-minimal-demo
```

Add under `examples/`:
```text
    tokio-minimal-demo.rs     ← binary: smallest runnable layered Tokio example (counter actor)
    tokio-pool-demo.rs        ← binary: wires pool/worker actors; injects spawn_worker from impl crate
```

**Final tree should include**:
```
bloxide/
  spec/                        ← architecture docs, blox specs, templates (READ FIRST)
    README.md                  ← spec directory guide and SDD workflow
    architecture/              ← system design, HSM engine, messaging, wiring
    bloxes/                    ← per-blox specs (ping, pong, ...)
    templates/                 ← blox-spec.md template for new bloxes
  skills/                      ← agent skills (workflows you should follow)
    building-with-bloxide/
      SKILL.md                 ← how to build bloxes with bloxide (portable — copy to downstream projects)
      reference.md             ← deep-dive companion: macro syntax, timer/supervision patterns, worked example
    contributing-to-bloxide/
      SKILL.md                 ← how to evolve the framework: engine, runtimes, stdlib crates, macros
  crates/
    bloxide-core/              ← HSM engine, BloxRuntime trait, channel traits (no_std)
    bloxide-log/               ← feature-gated logging macros (log / defmt backends); no_std
    bloxide-macros/            ← proc macros: #[derive(BloxCtx)], transitions!, #[blox_event], etc.
    bloxide-spawn/             ← dynamic actor support: SpawnCap, DynamicChannelCap (no_std)
    bloxide-timer/             ← timer library: TimerCommand, TimerId, TimerQueue, HasTimerRef, TimerService trait
    bloxide-supervisor/        ← generic reusable supervisor: SupervisorSpec, ChildGroup, ChildPolicy, GroupShutdown, LifecycleCommand
    messages/
      ping-pong-messages/      ← PingPongMsg shared by both ping and pong bloxes
      pool-messages/           ← PoolMsg, WorkerMsg, DoWork, WorkDone, etc. shared by pool and worker
      counter-messages/        ← CounterMsg shared by counter blox and minimal wiring demo
    actions/
      ping-pong-actions/       ← HasPeerRef, CountsRounds, send_initial_ping, send_pong, etc. (no concrete types)
      pool-actions/            ← WorkerSpawnFn, HasWorkers, HasWorkerFactory, HasCurrentTask, introduce_new_worker, etc.
      counter-actions/         ← CountsTicks behavior trait and increment_count action
    bloxes/
      ping/                    ← declarative Ping actor; depends on ping-pong-actions
      pong/                    ← declarative Pong actor; depends on ping-pong-actions
      worker/                  ← declarative Worker actor; depends on pool-actions (no pool-blox dependency)
      pool/                    ← declarative Pool actor; depends on pool-actions (no worker-blox dependency)
      counter/                 ← declarative Counter actor; depends on counter-actions
    impl/
      embassy-demo-impl/       ← impl crate: PingBehavior (concrete behavior for Ping)
      counter-demo-impl/       ← impl crate: CounterBehavior for tokio-minimal-demo
      tokio-pool-demo-impl/    ← impl crate: tokio worker factory for pool demo
  runtimes/
    bloxide-embassy/           ← Embassy runtime implementation
    bloxide-tokio/             ← Tokio runtime implementation; implements SpawnCap and DynamicChannelCap
  examples/
    embassy-demo.rs            ← binary: wires ping/pong actors and spawns Embassy tasks
    tokio-minimal-demo.rs      ← binary: smallest runnable layered Tokio example (counter actor)
    tokio-demo.rs              ← binary: wires ping/pong actors on Tokio
    tokio-pool-demo.rs         ← binary: wires pool/worker actors; injects spawn_worker from impl crate
  AGENTS.md                    ← this file
```

**Verification**: Compare tree output against actual `ls -R crates/` and `ls examples/`.

---

## Phase 2: High Priority Fixes

### 2.1 Document Macro Features in bloxide-macros

**File**: `crates/bloxide-macros/src/lib.rs`
**Lines**: 93-101 (documentation block for `#[derive(StateTopology)]`)

**Action**: Add the following to the documentation for `#[derive(StateTopology)]`:

Find the line that currently reads (approximately):
```rust
/// # Example
```

Before that section, add:
```rust
/// # Attributes
///
/// ## Enum-level: `#[handler_fns(...)]`
///
/// Specifies handler function names for each state variant. Auto-generates a companion
/// macro `{snake_case_state_enum_name}_handler_table!(Self)` for constructing the
/// `HANDLER_TABLE` const array.
///
/// Example:
/// ```ignore
/// #[derive(StateTopology)]
/// #[handler_fns(on_entry_state_a, on_exit_state_a, on_entry_state_b)]
/// enum State {
///     StateA,
///     StateB,
/// }
///
/// // Generated macro (call in impl MachineSpec):
/// // fn state_handler_table() -> &'static [StateFns<Self>] {
/// //     &state_handler_table!(Self)
/// // }
/// ```
```

**Action**: Add `#[ctor]` to the `#[derive(BloxCtx)]` documentation.

Find the section documenting field annotations. Add:
```rust
/// ## Field Annotation: `#[ctor]`
///
/// Marks a field as constructor-only. The field is set during `new()` construction
/// but does NOT generate an accessor trait impl. Used for fields that the runtime
/// injects (e.g., factory closures, spawn capabilities) that should not be exposed
/// as `HasXRef` accessor traits.
///
/// Example:
/// ```ignore
/// #[derive(BloxCtx)]
/// pub struct PoolCtx<R: BloxRuntime> {
///     #[self_id]
///     pub self_ref: ActorRef<PoolMsg, R>,
///     #[provides(WorkerRef)]
///     pub workers: Vec<ActorRef<WorkerMsg, R>>,
///     #[ctor]
///     spawn_worker: WorkerSpawnFn<R>,  // Injected at construction, no accessor trait
/// }
/// ```
```

**Verification**: Run `cargo doc --package bloxide-macros --open` and verify both features are documented.

---

### 2.2 Document `transitions!` Pattern Classification Rules

**File**: `crates/bloxide-macros/src/lib.rs`
**Lines**: In the `transitions!` macro documentation section

**Action**: Add the following section to the `transitions!` macro documentation:

```rust
/// # Pattern Classification Rules
///
/// The macro inspects pattern syntax to determine how to match events:
///
/// | Pattern Syntax | Generated Match | Use Case |
/// |----------------|-----------------|----------|
/// | `_Msg(Ping { .. })` | `event.msg_payload()` | Match message payloads from `Event::Msg(Envelope<T>)` |
/// | `_Ctrl(Stop)` | `event.ctrl_payload()` | Match control payloads from `Event::Ctrl(T)` |
/// | `Event::Msg(Envelope(Ping { .. }))` | Direct pattern match | Full explicit path |
/// | `Ping { .. }` (no suffix) | Direct struct pattern | Custom event types |
///
/// ## Important Naming Convention
///
/// For the shorthand patterns to work:
/// - Message events must have variants ending in `Msg` (e.g., `PingMsg`, `PongMsg`)
/// - Control events must have variants ending in `Ctrl` (e.g., `StopCtrl`, `ResetCtrl`)
///
/// **Common Pitfall**: If your event enum uses `PingMessage` instead of `PingMsg`,
/// the shorthand won't work. Either rename your variant or use the full path syntax.
```

**Verification**: Run `cargo doc --package bloxide-macros --open` and verify pattern rules are visible.

---

### 2.3 Add Prelude to bloxide-timer

**File**: `crates/bloxide-timer/src/prelude.rs` (NEW FILE)

**Action**: Create new file with:

```rust
//! Prelude for the `bloxide-timer` crate.
//!
//! Import with `use bloxide_timer::prelude::*;` for quick access to commonly used types.

pub use crate::actions::{cancel_timer, set_timer, HasTimerRef};
pub use crate::command::{next_timer_id, TimerCommand, TimerId, TIMER_ACTOR_ID};
pub use crate::queue::TimerQueue;
pub use crate::service::TimerService;
```

**File**: `crates/bloxide-timer/src/lib.rs`

**Action**: Add the prelude module export. In the module declarations section, add:

```rust
pub mod prelude;
```

**Verification**: 
```bash
cd crates/bloxide-timer && cargo build
grep -n "pub mod prelude" src/lib.rs  # Should find exactly one line
```

---

### 2.4 Add Prelude to bloxide-log

**File**: `crates/bloxide-log/src/prelude.rs` (NEW FILE)

**Action**: Create new file with:

```rust
//! Prelude for the `bloxide-log` crate.
//!
//! Import with `use bloxide_log::prelude::*;` for quick access to all logging macros.

pub use crate::{
    blox_log_debug, blox_log_error, blox_log_info, blox_log_trace, blox_log_warn,
};
```

**File**: `crates/bloxide-log/src/lib.rs`

**Action**: Add the prelude module export. In the module declarations section, add:

```rust
pub mod prelude;
```

**Verification**:
```bash
cd crates/bloxide-log && cargo build
grep -n "pub mod prelude" src/lib.rs  # Should find exactly one line
```

---

### 2.5 Add SpawnCap Invariant to AGENTS.md

**File**: `AGENTS.md`
**Lines**: After line 130 (after invariant #14)

**Action**: Add invariant #15:

```markdown
15. **Dynamic actor spawning via factory injection** — Blox crates never declare `R: SpawnCap`. Dynamic spawning uses factory injection via `#[ctor]` fields in blox context structs. The binary (or impl crate) provides the concrete factory closure at construction time. This keeps blox crates portable across all runtimes, including Embassy which lacks `SpawnCap`.
```

**Verification**:
```bash
grep -n "SpawnCap" AGENTS.md  # Should find at least 2 occurrences (-tier 2 table + new invariant)
```

---

### 2.6 Add SAFETY Comments for Debug-Only Invariant Checks

**File**: `crates/bloxide-core/src/engine.rs`
**Lines**: 169-173 (the `debug_assert!` for HANDLER_TABLE length)

**Action**: Replace the existing code with:

```rust
// SAFETY: The HANDLER_TABLE must have exactly STATE_COUNT entries.
// Violation of this invariant in release builds is undefined behavior:
// the engine will index out of bounds when dispatching events.
// The `#[derive(StateTopology)]` macro with `#[handler_fns(...)]` generates
// tables of correct length; manual construction must ensure the same.
debug_assert!(
    S::HANDLER_TABLE.len() == S::State::STATE_COUNT,
    "HANDLER_TABLE length ({}) must match STATE_COUNT ({}) — \
     use the generated {snake_case}_handler_table!() macro or \
     ensure manual construction is correct",
    S::HANDLER_TABLE.len(),
    S::State::STATE_COUNT
);
```

**File**: `crates/bloxide-core/src/topology.rs`
**Lines**: 83-89 (the `LeafState::new` debug_assert)

**Action**: Replace the existing code with:

```rust
/// Creates a new `LeafState` representing the given state.
///
/// # Safety (Internal)
///
/// The caller must ensure `state` is a leaf state (one with no children).
/// In debug builds, this is verified via `debug_assert!`. In release builds,
/// passing a non-leaf state is undefined behavior — the engine assumes
/// all active states and transition targets are leaves.
///
/// The `#[derive(StateTopology)]` macro only generates `LeafState` values
/// for leaf states; manual construction must ensure the same.
    debug_assert!(
    parent(state).is_none(),
    "LeafState::new called with non-leaf state {:?} — \
     only leaf states may be active or be transition targets",
    state
);
```

**Verification**:
```bash
grep -n "SAFETY" crates/bloxide-core/src/engine.rs
grep -n "SAFETY" crates/bloxide-core/src/topology.rs
# Each should find the new comments
```

---

### 2.7 Document Tier 2 Trait Naming Pattern

**File**: `spec/architecture/00-layered-architecture.md`

**Action**: In the Tier 2 traits section (around lines 80-110), add a naming convention subsection:

```markdown
#### Tier 2 Trait Naming Convention

| Suffix | When to Use | Examples |
|--------|-------------|----------|
| `*Service` | Async bridge traits that run a background task | `TimerService` |
| `*RunLoop` | Traits that merge multiple streams into an actor loop | `SupervisedRunLoop` |
| `*Cap` (Capability) | Traits that provide runtime capabilities for injection | `SpawnCap`, `StaticChannelCap`, `DynamicChannelCap` |

**Why different suffixes?**
- `*Service` traits are async services (like timer management)
- `*RunLoop` traits define actor execution patterns (supervision loop)
- `*Cap` traits are capabilities that runtimes implement for injection (spawning, channels)
```

**Verification**: Check that the naming convention section exists in the file.

---

### 2.8 Update QUICK_REFERENCE.md Timer Pattern

**File**: `QUICK_REFERENCE.md`
**Lines**: 93-108 (timer handling section)

**Action**: Replace the current timer pattern with:

```markdown
## Timer Pattern

Use `bloxide-timer` action functions instead of manual message construction.

### Setup

1. Add dependency:
   ```toml
   [dependencies]
   bloxide-timer = { version = "0.1", features = ["std"] }
   ```

2. Add timer fields to context:
   ```rust
   #[derive(BloxCtx)]
   pub struct MyCtx<R: BloxRuntime> {
       #[self_id]
       pub self_ref: ActorRef<MyMsg, R>,
       #[provides(TimerRef)]
       pub timer_ref: ActorRef<TimerCommand, R>,  // Required
   }
   ```

3. Implement `HasTimerRef` (auto-derived via `#[provides(TimerRef)]`).

### Setting a Timer

```rust
use bloxide_timer::{set_timer, next_timer_id, TimerCommand};

// In an action function:
fn start_timeout<R: BloxRuntime>(ctx: &mut MyCtx<R>, event: &MyEvent) {
    let timer_id = next_timer_id();
    set_timer(ctx, timer_id, Duration::from_secs(5), MyMsg::Timeout { id: timer_id });
}
```

### Canceling a Timer

```rust
use bloxide_timer::cancel_timer;

fn cancel_timeout<R: BloxRuntime>(ctx: &mut MyCtx<R>, timer_id: TimerId) {
    cancel_timer(ctx, timer_id);
}
```

### Handling Timer Expiration

```rust
transitions! {
    // Match the timer message by ID
    MyMsg::Timeout { id } if *id == expected_id => {
        actions: handle_timeout,
        guard: |ctx, results, event| Guard::Transition(LeafState::new(State::TimedOut)),
    }
}
```
```

**Verification**: Review the updated section in the file.

---

## Phase 3: Medium Priority Fixes

### 3.1 Hide TransitionRule from Public API

**File**: `crates/bloxide-core/src/lib.rs`
**Lines**: Around 25 (public exports)

**Action**: Find the line that exports both `StateRule` and `TransitionRule`:

```rust
pub use transition::{ActionResults, ActionResult, Guard, StateRule, Transition, TransitionRule};
```

Change to:

```rust
pub use transition::{ActionResults, ActionResult, Guard, StateRule, Transition};
// Note: TransitionRule is an implementation detail; use StateRule type alias instead.
// pub(crate) use transition::TransitionRule;  // Uncomment if needed internally
```

**File**: `crates/bloxide-core/src/transition.rs`
**Lines**: Around line 1

**Action**: Add a documentation comment to the `TransitionRule` struct:

```rust
/// Internal representation of a transition rule.
///
/// **Users should not name this type directly.** Use the `StateRule<S>` type alias instead,
/// which adds the `Guard<S>` type parameter.
///
/// This type is kept public for proc-macro generated code compatibility, but
/// the type alias is the preferred API surface.
```

**Verification**: Check that `StateRule` is the primary public type.

---

### 3.2 Document Accessor Trait Naming Pattern

**File**: `spec/architecture/12-action-crate-pattern.md`

**Action**: Add a new subsection to the " accessor traits" section:

```markdown
### Accessor Trait Naming Convention

| Name Pattern | Use Case | Example |
|---------------|----------|---------|
| `HasXRef` | Single reference access (singular) | `HasTimerRef`, `HasPeerRef` |
| `HasX` | Collection access (plural) | `HasChildren`, `HasWorkerPeers`, `HasWorkers` |

**Rule of thumb**: 
- If the accessor returns a single `ActorRef<M>`, name it `HasXRef`.
- If the accessor returns a collection (Vec, map, etc.), name it `HasX`.
```

**Verification**: The naming convention should be clearly documented.

---

### 3.3 Add Error Impls to TestRuntime Error Types

**File**: `crates/bloxide-core/src/test_utils.rs`
**Lines**: 100-101 (TestSendError and TestTrySendError definitions)

**Action**: Replace the error type definitions with:

```rust
/// Error returned by `TestRuntime::send_via` (always succeeds in test).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestSendError;

impl core::fmt::Display for TestSendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "test send error (should never occur)")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TestSendError {}

/// Error returned by `TestRuntime::try_send_via` when capacity is exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestTrySendError;

impl core::fmt::Display for TestTrySendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "test try_send error: channel full")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TestTrySendError {}
```

**Verification**: `cargo test --package bloxide-core` should pass.

---

### 3.4 Add Lifecycle Cross-References to DispatchOutcome

**File**: `crates/bloxide-core/src/engine.rs`
**Lines**: 124-138 (DispatchOutcome enum)

**Action**: Add documentation to `Started` and `Transition` variants:

```rust
/// The machine was started (Init state's on_entry executed).
///
/// **Runtime must check**: `MachineSpec::is_terminal()` on the Init state.
/// If terminal, the actor should exit immediately.
Started,

/// A transition occurred (current state changed).
///
/// **Runtime must check**: 
/// - `MachineSpec::is_terminal()` on the new state
/// - `MachineSpec::is_error()` on the new state
///
/// If `is_error()` returns true, report `ChildLifecycleEvent::Failed`.
/// If `is_terminal()` returns true (and not error), report `ChildLifecycleEvent::Done`.
Transition {
    from: LeafState<S::State>,
    to: LeafState<S::State>,
},
```

**Verification**: Check that the new documentation is present.

---

### 3.5 Consolidate Duplicate `reset` Explanations in Supervision Doc

**File**: `spec/architecture/08-supervision.md`

**Action**: 
1. Find the duplicate reset explanation at lines 165-180.
2. Replace it with a cross-reference:

```markdown
### Reset Behavior

When a child fails and the policy indicates `Reset`, the supervisor:

1. Sends `LifecycleCommand::Reset` to the child (triggers LCA exit chain)
2. Waits for the child to notify `Reset`
3. Sends `LifecycleCommand::Start` to restart the child from Init state

For detailed mechanics of the LCA exit chain during reset, see the
[Reset and LCA Exit Chain](#reset-and-lca-exit-chain) section below.
```

3. Verify the section at lines 245-265 is labeled `### Reset and LCA Exit Chain`.

**Verification**: There should be only one detailed reset explanation.

---

### 3.6 Standardize Export Pattern Across Stdlib Crates

**File**: `crates/bloxide-spawn/src/lib.rs`
**Lines**: 12-13

**Action**: Replace star exports with explicit re-exports:

```rust
// Before:
// pub use capability::*;
// pub use peer::*;

// After:
pub use capability::SpawnCap;
// peer module removed - domain-specific types instead
pub use spawn_introduce::introduce_peers;
pub use spawn_test_impl::{drain_spawned, spawned_count};

#[cfg(feature = "std")]
pub use spawn_test_impl::{test_impl, SpawnContext};
```

**Verification**: `cargo doc --package bloxide-spawn` should list explicit re-exports.

---

## Phase 4: Low Priority Fixes

### 4.1 Document ActionResult Error Discard Behavior

**File**: `crates/bloxide-core/src/transition.rs`
**Lines**: 18-25

**Action**: Add documentation to the `From<Result<(), E>>` impl:

```rust
/// Converts a `Result<(), E>` into an `ActionResult`.
///
/// **Note**: Error details are discarded. The guard receives only the
/// `any_failed()` boolean via `ActionResults`. If you need to preserve
/// error information for the guard, store it in the context before returning.
impl<E> From<Result<(), E>> for ActionResult {
    fn from(r: Result<(), E>) -> Self {
        if r.is_ok() { ActionResult::Ok } else { ActionResult::Err }
    }
}
```

---

### 4.2 Add Clone/Copy to NoMailboxes

**File**: `crates/bloxide-core/src/mailboxes.rs`
**Lines**: 42-48

**Action**: Update the struct definition:

```rust
/// Zero-sized type indicating no typed mailboxes are needed.
///
/// Used as the `Mailboxes` associated type for actors that only handle
/// lifecycle commands (supervisors that don't receive domain messages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoMailboxes;
```

---

### 4.3 Extract EventTag Limit to a Constant

**File**: `crates/bloxide-macros/src/event_tag.rs`
**Lines**: 34-38

**Action**: Define a constant:

```rust
/// Maximum number of event variants supported by the EventTag derive.
/// Limited to 254 to reserve 0 and 255 for internal use.
pub const MAX_EVENT_VARIANTS: usize = 254;
```

Use this constant instead of the hardcoded `254` value.

**File**: `crates/bloxide-macros/src/blox_event.rs`
**Lines**: 14-18

**Action**: Use the same constant:

```rust
use crate::event_tag::MAX_EVENT_VARIANTS;

// Replace hardcoded 254 with MAX_EVENT_VARIANTS
```

---

### 4.4 Standardize Spec-Driven Development Workflow

**Files to update**:
1. `AGENTS.md` lines 135-140
2. `skills/building-with-bloxide/SKILL.md` lines 116-124

**Action**: Standardize all three files to use the same 5-step workflow:

```markdown
## Spec-Driven Development Workflow

1. **Spec first** — Write/update `spec/bloxes/<name>.md` with state diagram, events, transitions
2. **Tests next** — Write `TestRuntime`-based tests per acceptance criteria
3. **Then code** — Implement `MachineSpec` to pass tests
4. **Review** — Verify impl matches spec; update tests if gaps found
5. **Keep in sync** — Update spec diagrams if implementation reveals spec errors
```

---

## Phase 5: Architecture Doc Fixes

### 5.1 Add SpawnCap to Tier 2 Implementation Map

**File**: `spec/architecture/00-layered-architecture.md`
**Lines**: 96-106 (Tier 2 Implementation Map table)

**Action**: Add a row for `SpawnCap`:

```markdown
| Trait | Embassy | Tokio | Description |
|-------|---------|-------|-------------|
| `StaticChannelCap` | ✅ | ✅ | Static-capacity channel creation |
| `DynamicChannelCap` | ❌ | ✅ | Dynamic-capacity channel creation |
| `TimerService` | ✅ | ✅ | Timer service run loop |
| `SupervisedRunLoop` | ✅ | ✅ | Supervised actor run loop |
| `SpawnCap` | ❌ | ✅ | Dynamic actor spawning |
```

---

### 5.2 Add Cross-Reference from Handler Patterns to Invariant

**File**: `spec/architecture/05-handler-patterns.md`
**Lines**: Around line 76

**Action**: Add after the bubbling description:

```markdown
> See `AGENTS.md` invariant #8 for the formal constraint: never add a catch-all
> rule that manually returns a parent; bubbling is implicit.
```

---

### 5.3 Consolidate Duplicate "Related Docs" Sections

**File**: `spec/architecture/12-action-crate-pattern.md`
**Lines**: 172-179

**Action**: Remove the "Related Docs" section at lines 172-179.
Keep only the one at lines 11-14.

---

## Verification Checklist

After implementing all phases, run the following verification:

```bash
# Build all crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Check documentation builds
cargo doc --workspace --no-deps

# Verify no_std compatibility
cargo build --target thumbv7em-none-eabihf --workspace --exclude bloxide-tokio

# Check clang format (if configured)
cargo fmt -- --check

# Verify specific changes
grep -r "Base trait for runtime-specific" crates/bloxide-core/src/capability.rs
grep -r "Dynamic actor spawning via factory injection" AGENTS.md
grep -r "pub mod prelude" crates/bloxide-timer/src/lib.rs
grep -r "pub mod prelude" crates/bloxide-log/src/lib.rs
```

---

## Summary Table

| Phase | Items | Risk | Estimated Effort |
|-------|-------|------|-------------------|
| 1. Critical Docs | 3 | None | 1-2 hours |
| 2. High Priority | 8 | Low | 3-4 hours |
| 3. Medium Priority | 6 | Low | 2-3 hours |
| 4. Low Priority | 4 | None | 1 hour |
| 5. Architecture Docs | 3 | None | 30 min |

**Total Estimated Effort**: 8-11 hours

---

## File Change Summary

### New Files
- `crates/bloxide-timer/src/prelude.rs`
- `crates/bloxide-log/src/prelude.rs`

### Modified Files
- `AGENTS.md`
- `QUICK_REFERENCE.md`
- `spec/architecture/00-layered-architecture.md`
- `spec/architecture/05-handler-patterns.md`
- `spec/architecture/08-supervision.md`
- `spec/architecture/12-action-crate-pattern.md`
- `crates/bloxide-core/src/lib.rs`
- `crates/bloxide-core/src/capability.rs`
- `crates/bloxide-core/src/engine.rs`
- `crates/bloxide-core/src/topology.rs`
- `crates/bloxide-core/src/transition.rs`
- `crates/bloxide-core/src/mailboxes.rs`
- `crates/bloxide-core/src/test_utils.rs`
- `crates/bloxide-macros/src/lib.rs`
- `crates/bloxide-macros/src/event_tag.rs`
- `crates/bloxide-macros/src/blox_event.rs`
- `crates/bloxide-timer/src/lib.rs`
- `crates/bloxide-log/src/lib.rs`
- `crates/bloxide-spawn/src/lib.rs`
