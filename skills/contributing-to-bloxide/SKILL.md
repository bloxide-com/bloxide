---
name: contributing-to-bloxide
description: Guide for modifying the bloxide framework itself (HSM engine, proc macros, standard library crates, and runtime implementations). Use when adding new capabilities to the framework, implementing new runtimes, or modifying core engine behavior.
metadata:
  short-description: Evolve Bloxide framework core/runtimes
---

# Contributing to Bloxide

This guide is for modifying the bloxide framework itself — the HSM engine, proc macros, standard library crates, and runtime implementations. If you are building bloxes (actors) with bloxide, read `skills/building-with-bloxide/SKILL.md` instead.

## Crate Map

```
bloxide-core        HSM engine, BloxRuntime, channel traits, TestRuntime     (no_std)
bloxide-macros      Proc macros: BloxCtx, StateTopology, transitions!        (host-compiled)
bloxide-log         Feature-gated logging macros                              (no_std)
bloxide-timer       Timer service: commands, queue, accessor traits           (no_std)
bloxide-supervisor  Reusable supervisor: ChildGroup, policies, run loop       (no_std)
bloxide-spawn       Dynamic actor spawning and peer introduction              (no_std)
bloxide-embassy     Embassy runtime: channels, tasks, timer bridge            (no_std)
bloxide-tokio       Tokio runtime: channels, tasks, SpawnCap                  (std)
```

**Dependency direction:** `bloxide-core` is the root. Standard library crates depend on `bloxide-core`. Runtime crates depend on `bloxide-core` + standard library crates. Domain crates (bloxes) depend only on `bloxide-core` and standard library crates — never on runtime crates.

## Two-Tier Trait System

### Tier 1 — Blox-facing

Blox crates see only this:

- `BloxRuntime` — the sole trait bloxes are generic over

```rust
pub trait BloxRuntime: Sized + Clone {
    type Sender<M: Send>: Clone + Send;
    type Receiver<M: Send>: Send;
    type Stream<M: Send>: Stream<Item = Envelope<M>> + Send;
    
    fn to_stream<M: Send>(rx: Self::Receiver<M>) -> Self::Stream<M>;
    fn send_via<M: Send>(tx: &Self::Sender<M>, msg: Envelope<M>) -> Result<(), SendError>;
    fn try_send_via<M: Send>(tx: &Self::Sender<M>, msg: Envelope<M>) -> Result<(), TrySendError>;
}
```

Blox crates never use Tier 2 traits as bounds.

### Tier 2 — Runtime-facing

These traits formalize the contract runtimes must fulfill:

| Trait | Crate | Purpose |
|-------|-------|---------|
| `StaticChannelCap` | `bloxide-core` | Compile-time capacity channel creation (used by `channels!` macro) |
| `DynamicChannelCap` | `bloxide-core` | Runtime-configurable channel creation (used by `TestRuntime`) |
| `TimerService` | `bloxide-timer` | Timer service run loop; bridges `TimerQueue` to native timer |
| `SupervisedRunLoop` | `bloxide-supervisor` | Supervised actor run loop; merges lifecycle with domain mailboxes |
| `SpawnCap` | `bloxide-spawn` | Dynamic actor spawning; extends `DynamicChannelCap` |

When adding a new capability, decide which tier it belongs to. If blox crates need it, it is Tier 1 (accessor traits, action functions). If only runtimes implement it, it is Tier 2 (service trait).

## Key Invariants for Framework Code

1. **`bloxide-core` is `no_std`** — zero OS, Tokio, or Embassy imports. `futures-core` is the only always-on runtime dep.
2. **`on_entry`/`on_exit` are infallible** — they are `fn(&mut Ctx)` with no `Result`.
3. **Actions before guards** — `guard` receives `&Ctx` and `&ActionResults`, not `&mut Ctx`.
4. **Only leaf states as transition targets** — the engine `debug_assert`s this.
5. **Lifecycle commands flow through dispatch()** — actors handle them as domain events via `root_transitions()`.
6. **`is_error` takes precedence over `is_terminal`** — if both return `true`, supervisor reports `Failed`, not `Done`.
7. **KillCap immediately aborts** — no callbacks fire, task is dropped in-place.

## Adding a Standard Library Crate

Follow the `bloxide-timer` / `bloxide-supervisor` / `bloxide-spawn` pattern.

### 1. Create the crate

```toml
# crates/bloxide-<name>/Cargo.toml
[package]
name = "bloxide-<name>"
version.workspace = true
edition.workspace = true

[dependencies]
bloxide-core = { workspace = true }
```

The crate must be `#![no_std]`. Use `extern crate alloc` if heap allocation is needed.

### 2. Define blox-facing side

- **Command/message types** — plain data enums/structs
- **Shared data structures** — types both bloxes and runtimes use
- **Accessor traits** — `HasXRef<R>` providing access to `ActorRef`s
- **Action functions** — generic, trait-bounded functions

### 3. Define runtime-facing side

A service trait extending `BloxRuntime`:

```rust
pub trait MyService: BloxRuntime {
    fn run_my_service(queue: MyQueue) -> impl Future<Output = ()>;
}
```

This is a Tier 2 trait — blox crates never use it as a bound.

## Adding a Runtime

### 1. Implement `BloxRuntime` and Tier 2 traits

```rust
// runtimes/bloxide-myrt/src/lib.rs
#![no_std]  // or #![no_std] with extern crate alloc, or std

pub struct MyRuntime;

impl BloxRuntime for MyRuntime {
    type Sender<M: Send> = MySender<M>;
    type Receiver<M: Send> = MyReceiver<M>;
    type Stream<M: Send> = MyStream<M>;
    
    fn to_stream<M: Send>(rx: Self::Receiver<M>) -> Self::Stream<M> { ... }
    fn send_via<M: Send>(tx: &Self::Sender<M>, msg: Envelope<M>) -> Result<(), SendError> { ... }
    fn try_send_via<M: Send>(tx: &Self::Sender<M>, msg: Envelope<M>) -> Result<(), TrySendError> { ... }
}

impl StaticChannelCap for MyRuntime { ... }
impl DynamicChannelCap for MyRuntime { ... }
```

### 2. Provide runtime-specific macros

```rust
// Channel creation
#[macro_export]
macro_rules! channels {
    ($($name:ident ($cap:literal)),* $(,)?) => { ... }
}

// Actor task spawning
#[macro_export]
macro_rules! actor_task_supervised {
    ($name:ident, $spec:ty) => { ... }
}

// Timer spawning (if applicable)
#[macro_export]
macro_rules! spawn_timer {
    ($cap:literal) => { ... }
}
```

### 3. Export a prelude

```rust
pub mod prelude {
    pub use bloxide_core::prelude::*;
    pub use bloxide_core::{BloxRuntime, StaticChannelCap, DynamicChannelCap};
    pub use crate::{channels, spawn_child, actor_task_supervised, ...};
}
```

## Proc Macro Guidelines

Proc macros live in `bloxide-macros` (host-compiled, exempt from `no_std`).

### Generated Code Must:

1. Use only types re-exported from `bloxide-core` (not runtime crates)
2. Reference `R: BloxRuntime` bounds for runtime-generic types
3. Generate `const` items for handler tables (`StateFns`)

### Key Macros:

- `#[derive(BloxCtx)]` — generates accessor impls and constructor
- `#[derive(StateTopology)]` — generates topology helpers and handler table macro
- `transitions!` — builds `&'static [StateRule<S>]` slices
- `root_transitions!` — builds root-level fallback rules
- `event!` — generates event enum with payload helpers
- `blox_messages!` — generates message enum with named struct variants

### Macro Testing

Test proc macros via `#[cfg(test)]` in the macro crate:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blox_ctx_generation() {
        // Expand macro and check output
    }
}
```

## Engine Invariants

### Dispatch Loop

1. Receive event from merged mailbox stream
2. Check for `LifecycleCommand` → handle at VirtualRoot level first
3. Walk state path from leaf to root, checking each state's `transitions`
4. If no match, bubble to parent (implicit, no catch-all rules)
5. If `guard` returns `Transition(target)`, run exit chain → entry chain
6. Report `DispatchOutcome` for supervisor notifications

### State Transitions

- `Transition(target)` — run exit chain from current leaf to LCA, then entry chain from LCA to target
- `Stay` — no callbacks
- `Reset` — run full exit chain, enter engine-implicit Init, call `on_init_entry`
- `Fail` — same as Reset, but report `Failed` to supervisor

### Lifecycle Flow

```
Init --Start--> initial_state()
Any --Reset--> Init (on_init_entry called)
Any --Stop--> Init (suspended, can restart)
Any --Kill--> abort immediately (permanent death)
```

## Testing Guidelines

### TestRuntime

Located in `bloxide-core/src/test_utils.rs`. Provides:
- In-memory channels with `try_send`/`drain` 
- `alloc_actor_id()` for unique IDs
- No async executor needed

### VirtualClock

Located in `bloxide-timer/src/test_utils.rs`. Provides:
- Manual time advancement
- Fires timers when duration elapsed
- No native timer needed

### Test Pattern

```rust
#[cfg(all(test, feature = "std"))]
mod tests {
    use bloxide_core::test_utils::TestRuntime;
    use bloxide_core::{spec::MachineSpec, MachineState, StateMachine};

    fn make_machine() -> StateMachine<MySpec<TestRuntime, TestBehavior>> {
        let ctx = MyCtx::new(bloxide_core::next_actor_id!(), TestBehavior::default());
        StateMachine::new(ctx)
    }

    #[test]
    fn test_basic_transition() {
        let mut machine = make_machine();
        machine.dispatch(MyEvent::Lifecycle(LifecycleCommand::Start));
        assert!(matches!(machine.current_state(), MachineState::State(MyState::Ready)));
    }
}
```

## Adding New Capabilities Checklist

1. **Does it require async waiting?**
   - No → context field, direct access
   - Messages only → standard run loop
   - External async source → standard library crate

2. **Which tier?**
   - Blox crates need it → Tier 1 (accessor traits, action functions)
   - Only runtimes → Tier 2 (service trait)

3. **Messages required?**
   - Yes → add to existing `*-messages` or create new crate
   - No → skip message layer

4. **Mutable state?**
   - Yes → behavior trait with `#[delegatable]`
   - No → accessor trait only

5. **Runtime support?**
   - Yes → service trait in stdlib crate, impl in each runtime
   - No → pure `bloxide-core` types suffice
