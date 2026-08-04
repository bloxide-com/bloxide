---
name: contributing-to-bloxide
description: Guide for modifying the bloxide framework itself (HSM engine, proc macros, standard library crates, and runtime implementations). Use when adding new capabilities to the framework, implementing new runtimes, or modifying core engine behavior.
metadata:
  short-description: Evolve Bloxide framework core/runtimes
---

# Contributing to Bloxide

This guide is for modifying the bloxide framework itself — the HSM engine, proc macros, standard library crates, and runtime implementations. If you are building bloxes (actors) with bloxide, read `skills/building-with-bloxide/SKILL.md` instead.

> **NO BACKWARDS COMPATIBILITY.** This project does not preserve backwards
> compatibility — not for APIs, CLIs, config formats, terminology, or
> architecture layers. When something changes, update every call site, every
> doc, and every fixture in the same change, and delete the old form
> completely. Never add legacy aliases, deprecation periods, compatibility
> shims, feature bridges, or "kept for backwards compatibility" notes.

## Crate Map

```
bloxide-core        HSM engine, BloxRuntime, channel traits, KillCapability, run/RunConfig  (no_std)
bloxide-macros      Proc macros: channels!, dyn_channels!, next_actor_id!     (host-compiled)
bloxide-codegen     TOML-driven code generator library                        (host-compiled)
cargo-blox          CLI: see QUICK_REFERENCE.md → "cargo blox Command Reference"  (host-compiled)
bloxide-log         Feature-gated logging macros                              (no_std)
bloxide-timer       Timer service: commands, queue, timer action functions     (no_std)
bloxide-spawn       Spawn capability: SpawnCap, ChildRegistrar, spawn_child   (no_std)
bloxide-child-management  Child tracking: ChildGroup, ChildGroupBuilder, ChildPolicy, GroupShutdown  (no_std)
bloxide-supervisor  Supervisor blox: SupervisorSpec, ChildCtrl, actions  (no_std)
bloxide-peers       Peer introduction: PeerCtrl, AddPeer, RemovePeer, introduce_peers, apply_peer_control, broadcast_to_peers  (no_std)
blox-ctx-ping-pong   Messaging helpers: send_ping, send_pong, send_initial_ping, schedule_resume  (no_std)
bloxide-embassy     Embassy runtime: channels, tasks, timer bridge            (no_std)
bloxide-tokio       Tokio runtime: channels, tasks, SpawnCap, KillCapability  (std)
```

**Dependency direction:** `bloxide-core` is the root. Standard library crates depend on `bloxide-core`. Runtime crates depend on `bloxide-core` + standard library crates. Domain crates (bloxes) depend only on `bloxide-core` and standard library crates — never on runtime crates.

## Two-Tier Trait System

### Tier 1 — Blox-facing

Blox crates see only this:

- `BloxRuntime` — the sole trait bloxes are generic over

```rust
pub trait BloxRuntime: Clone + Send + 'static {
    type SendError: Debug + Send + 'static;
    type TrySendError: Debug + Send + 'static;
    type Sender<M: Send + 'static>: Clone + Send + Sync + 'static;
    type Receiver<M: Send + 'static>: Send + 'static;
    type Stream<M: Send + 'static>: Stream<Item = Envelope<M>> + Unpin + Send + 'static;
    type Kill: KillCapability<Self>;

    fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M>;
    async fn send_via<M: Send + 'static>(tx: &Self::Sender<M>, msg: Envelope<M>) -> Result<(), Self::SendError>;
    fn try_send_via<M: Send + 'static>(tx: &Self::Sender<M>, msg: Envelope<M>) -> Result<(), Self::TrySendError>;
    async fn yield_now() {}  // no-op default; runtimes override
}
```

(Simplified — see `crates/bloxide-core/src/capability.rs` for the exact definition.)

Blox crates never use Tier 2 traits as bounds.

### Tier 2 — Runtime-facing

These traits formalize the contract runtimes must fulfill:

| Trait | Crate | Purpose |
|-------|-------|---------|
| `StaticChannelCap` | `bloxide-core` | Compile-time capacity channel creation (used by `channels!` macro) |
| `DynamicChannelCap` | `bloxide-core` | Runtime-configurable channel creation (used by `TestRuntime`) |
| `TimerService` | `bloxide-timer` | Timer service run loop; bridges `TimerQueue` to native timer |
| `run` + `RunConfig` | `bloxide-core` | Unified actor run loop (root/supervised/unsupervised/bare); merges lifecycle with domain mailboxes |
| `SpawnCap` | `bloxide-spawn` | Dynamic actor spawning; extends `DynamicChannelCap` |
| `KillCapability` | `bloxide-core` | Immediately aborts actor tasks for dynamic actor cleanup |

When adding a new capability, decide which tier it belongs to. If blox crates need it, it is Tier 1 (plain context fields, action functions). If only runtimes implement it, it is Tier 2 (service trait).

## Key Invariants for Framework Code

The canonical invariant list lives in `AGENTS.md` → "Key Invariants" — it
applies to framework code too. Framework-specific reminders:

- `bloxide-core` stays `no_std` with zero OS/executor imports
- Lifecycle commands flow through `dispatch()` at VirtualRoot level
- `is_error` states report `Failed`; actors self-suspend via `Decision::Stop` or self-terminate cleanly via `Decision::Done` (task ends, supervisor deregisters)
- `KillCapability::kill` fires no callbacks — the task is dropped in-place

## Adding a Standard Library Crate

Follow the `bloxide-timer` / `bloxide-supervisor` / `bloxide-peers` pattern.

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
- **Action functions** — generic, runtime-bounded functions that take concrete params

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

- `bloxide-codegen` — TOML-driven code generator for messages, events, topology, and mailbox impls; emits `StateRule` struct literals from `[[topology.transitions]]` entries
- `cargo-blox` — CLI tool (`cargo blox ...`); full subcommand list: `QUICK_REFERENCE.md` → "cargo blox Command Reference"

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
- `Reset` — go **directly** to `initial_state()` (full exit chain + entry chain); skips Init entirely — `on_init_entry`/`on_init_exit` do NOT fire
- `Stop` — full exit chain, enter engine-implicit Init, call `on_init_entry`; actor is suspended until a `Start`
- `Fail` — transition to `error_state()` if declared, otherwise Init; report `Failed` to supervisor

### Lifecycle Flow

```
Init --Start--> initial_state() (on_init_exit fires)
Any  --Reset--> initial_state() (skips Init; no on_init_entry/exit)
Any  --Stop-->  Init (on_init_entry fires; suspended, can restart)
Any  --Kill-->  abort immediately (permanent death)
```

## Testing Guidelines

### TestRuntime

Located in `runtimes/bloxide-test-runtime/src/lib.rs`. Provides:
- In-memory channels with `try_send`/`drain` 
- `alloc_actor_id()` for unique IDs
- `SpawnCap` for dynamic-spawn wiring — handles are `usize` spawn ids; `kill` records the id in a thread-local log (TestRuntime does not destroy tasks), asserted via `drain_killed()` / `kill_count()`
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
    use bloxide_test_runtime::TestRuntime;
    use bloxide_core::{spec::MachineSpec, MachineState, StateMachine};

    fn make_machine() -> StateMachine<MySpec<TestRuntime>> {
        let ctx = MyCtx::new(bloxide_core::next_actor_id!());
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
   - Blox crates need it → Tier 1 (plain context fields, action functions)
   - Only runtimes → Tier 2 (service trait)

3. **Messages required?**
   - Yes → add to existing `*-messages` or create new crate
   - No → skip message layer

4. **Mutable state?**
   - Yes → plain field on the context struct, action functions take `&mut` to it
   - No → read-only field on the context struct

5. **Runtime support?**
   - Yes → service trait in stdlib crate, impl in each runtime
   - No → pure `bloxide-core` types suffice
