# Contributing to Bloxide

This guide is for agents modifying the bloxide framework itself — the HSM engine, proc macros, standard library crates, and runtime implementations. If you are building bloxes (actors) with bloxide, read `skills/building-with-bloxide/SKILL.md` instead.

## Crate Map

```
bloxide-core        HSM engine, BloxRuntime, channel traits, TestRuntime     (no_std)
bloxide-macros      Proc macros: #[derive(BloxCtx)], transitions!, etc.      (host-compiled)
bloxide-log         Feature-gated logging macros                              (no_std)
bloxide-timer       Timer service: commands, queue, accessor traits           (no_std)
bloxide-supervisor  Reusable supervisor: ChildGroup, policies, run loop       (no_std)
bloxide-spawn       Dynamic actor spawning and peer introduction              (no_std)
bloxide-embassy     Embassy runtime: channels, tasks, timer bridge            (no_std)
bloxide-tokio       Tokio runtime: channels, tasks, SpawnCap                  (std)
```

Dependency direction: `bloxide-core` is the root. Standard library crates depend on `bloxide-core`. Runtime crates depend on `bloxide-core` + standard library crates. Domain crates (bloxes) depend only on `bloxide-core` and standard library crates — never on runtime crates.

## Two-Tier Trait System

Traits serve two audiences. This is the core architectural pattern.

### Tier 1 — Blox-facing

Blox crates see only this:

- `BloxRuntime` (in `bloxide-core`) — the sole trait bloxes are generic over. Defines `Sender`, `Receiver`, `Stream`, `to_stream`, `send_via`, `try_send_via`.

Blox crates never use Tier 2 traits as bounds.

### Tier 2 — Runtime/wiring-facing

These traits formalize the contract runtimes must fulfill. They enable trait-qualified dispatch in macros and give compile-time errors if a runtime forgets to implement a required service.

| Trait | Crate | Purpose |
|---|---|---|
| `StaticChannelCap` | `bloxide-core` | Compile-time capacity channel creation (used by `channels!` macro) |
| `DynamicChannelCap` | `bloxide-core` | Runtime-configurable channel creation (used by `TestRuntime`) |
| `TimerService` | `bloxide-timer` | Timer service run loop; bridges `TimerQueue` to native timer |
| `SupervisedRunLoop` | `bloxide-supervisor` | Supervised actor run loop; merges lifecycle with domain mailboxes |
| `SpawnCap` | `bloxide-spawn` | Dynamic actor spawning; extends `DynamicChannelCap` |

When adding a new capability, decide which tier it belongs to. If blox crates need it, it is Tier 1 (accessor traits, action functions). If only runtimes implement it, it is Tier 2 (service trait).

## Decision Rule for New Capabilities

When adding something new, ask: **does it require async waiting on something other than domain messages?**

- **No** (synchronous, pure computation) — add it as a context field. Handlers use it directly.
- **Messages only** (domain actors) — use the standard run loop (`run_root` / `run_supervised_actor`).
- **Messages + external async source** (timers, UART, network) — create a standard library crate with:
  - Blox-facing: message types, accessor traits, action functions, shared data structures
  - Runtime-facing: service trait that runtimes implement to bridge their native primitives

## Adding a Standard Library Crate

Follow the `bloxide-timer` / `bloxide-supervisor` / `bloxide-spawn` pattern.

### 1. Create the crate

```
crates/bloxide-<name>/
  Cargo.toml          # depends on bloxide-core; no_std
  src/
    lib.rs
    prelude.rs        # re-exports for glob import
```

The crate must be `#![no_std]`. Use `extern crate alloc` if heap allocation is needed.

### 2. Define the blox-facing side

These are what blox crates import:

- **Command/message types** — plain data enums/structs (e.g., `TimerCommand`, `TimerId`)
- **Shared data structures** — types that both bloxes and runtimes use (e.g., `TimerQueue`)
- **Accessor traits** — `HasXRef<R>` providing access to `ActorRef`s for the service
- **Action functions** — generic, trait-bounded functions (e.g., `set_timer`, `cancel_timer`)

### 3. Define the runtime-facing side

A service trait extending `BloxRuntime`:

```rust
pub trait MyService: BloxRuntime {
    // Methods the runtime must implement
    fn run_my_service(queue: MyQueue, /* ... */) -> impl Future<Output = ()>;
}
```

This is a Tier 2 trait — blox crates never use it as a bound.

### 4. Update the dependency graph

- The new crate depends on `bloxide-core`
- Runtime crates add the new crate as a dependency and implement the service trait
- Update `spec/architecture/00-layered-architecture.md` dependency graph

### 5. Implement in each runtime

Each runtime crate implements the service trait, bridging native primitives:

```rust
// In bloxide-tokio:
impl MyService for TokioRuntime {
    fn run_my_service(queue: MyQueue) -> impl Future<Output = ()> {
        // Bridge to tokio::time, tokio::net, etc.
    }
}
```

### 6. Add wiring macros (if needed)

Runtime crates may provide macros for convenient wiring (e.g., `spawn_timer!`, `spawn_child!`).

## Implementing a New Runtime

A runtime crate must implement:

### Required (Tier 2 from `bloxide-core`)

- `BloxRuntime` — associated types `Sender<M>`, `Receiver<M>`, `Stream<M>`; methods `to_stream`, `send_via`, `try_send_via`
- `StaticChannelCap` — `fn channel<M>(cap: usize) -> (Sender<M>, Receiver<M>)`

### Required (Tier 2 from standard library crates)

- `TimerService` — bridge `TimerQueue` to native timers
- `SupervisedRunLoop` — merge lifecycle commands with domain mailboxes in the actor run loop

### Optional

- `DynamicChannelCap` — for runtimes supporting dynamic actor creation
- `SpawnCap` — for runtimes supporting task spawning at runtime (extends `DynamicChannelCap`)

### Provide run functions

- `run_root(machine, mailboxes)` — top-level actor run loop
- `run_supervised_actor(machine, mailboxes, lifecycle_rx)` — supervised actor run loop

### Provide wiring macros

- `channels!` — create typed channel tuples
- `actor_task!` / `actor_task_supervised!` — spawn actor tasks
- `spawn_child!` — register a child with a `ChildGroupBuilder`
- `spawn_timer!` — start the timer service task
- `next_actor_id!` — generate unique actor IDs

## Modifying the HSM Engine

The engine lives in `bloxide-core/src/`. Key types:

- `StateMachine<S: MachineSpec>` — owns `Ctx`, tracks current state and phase (Init/Operational)
- `MachineSpec` — trait defining state topology, handlers, initial state, terminal/error detection
- `StateFns<S>` — `on_entry`/`on_exit` action slices + `transitions` rules for one state
- `TransitionRule<S, G>` — `event_tag` + `matches` + `actions` + `guard`
- `Guard<S>` — `Transition(LeafState)`, `Stay`, or `Reset`

### Dispatch algorithm

Events flow: current state rules (first match wins) → parent state rules (bubble) → root rules → silently dropped.

### LCA transitions

`change_state(source, target)` builds root-first paths for both states, finds the Lowest Common Ancestor, exits source-side states leaf-first (not including LCA), enters target-side states root-first.

### Key invariants for engine changes

- Only leaf states may be active or be transition targets (`debug_assert` on `LeafState::new`)
- `on_entry`/`on_exit` are infallible (`fn(&mut Ctx)`)
- Run-to-completion: entire dispatch completes before next message
- `Reset` triggers full exit chain (leaf → virtual root) then `on_init_entry`
- `is_error` takes precedence over `is_terminal`

## Proc Macros (`bloxide-macros`)

Proc macros compile for the host. They have no `no_std` impact on the target binary.

| Macro | Input | Output |
|---|---|---|
| `#[derive(StateTopology)]` | State enum with `#[handler_fns(...)]` | `parent()`, handler table macro, `state_count()` |
| `#[derive(BloxCtx)]` | Context struct with field annotations | Accessor trait impls, `fn new(...)` constructor |
| `#[derive(EventTag)]` | Event enum | `EventTag` impl with discriminant tags |
| `transitions!` | Pattern-match DSL | `&'static [StateRule<S>]` with `TransitionRule` structs |
| `root_transitions!` | Same DSL | Same output, for `root_transitions()` |
| `#[blox_event]` | Event enum | `EventTag` + `From<Envelope<M>>` + `msg_payload()` |
| `#[delegatable]` | Trait definition | Companion `__delegate_TraitName!()` macro |

When modifying macros, test with `cargo expand` on an example blox to verify generated code. The macros must produce code that compiles under `no_std`.

## `no_std` Enforcement

`bloxide-core` and all standard library crates are `#![no_std]`. Rules:

- No `std::` imports. Use `core::` and `alloc::` (with `extern crate alloc` when needed).
- Only `futures-core` is allowed as a runtime library dependency in `bloxide-core`.
- CI checks `no_std` compilation. Run `cargo build --target thumbv7em-none-eabihf` to verify.
- `bloxide-macros` is exempt (proc macros are host-compiled).

## Crate Dependency Rules

```
bloxide-core (BloxRuntime, StaticChannelCap, DynamicChannelCap, HSM engine)
  └── bloxide-macros (proc macros; host-only)

bloxide-log (standalone; no dependency on bloxide-core)

bloxide-timer (depends on bloxide-core)
bloxide-supervisor (depends on bloxide-core)
bloxide-spawn (depends on bloxide-core)

bloxide-embassy (depends on bloxide-core, bloxide-timer, bloxide-supervisor)
bloxide-tokio (depends on bloxide-core, bloxide-timer, bloxide-supervisor, bloxide-spawn)
```

Never introduce cycles. Standard library crates depend on `bloxide-core` only. Runtime crates depend on `bloxide-core` + standard library crates. Blox crates depend on `bloxide-core` + standard library crates + `bloxide-macros` + `bloxide-log`. Blox crates never depend on runtime crates.

## Spec Maintenance

Architecture docs live in `spec/architecture/`. Per-blox specs live in `spec/bloxes/`.

When modifying the framework:

1. **Update affected architecture docs** — if you change the engine, update `02-hsm-engine.md`. If you add a capability, update `00-layered-architecture.md` (dependency graph) and add a new architecture doc if the capability is substantial.
2. **Update blox specs** — if engine changes affect how bloxes define states or handlers, update `spec/bloxes/*.md` and `spec/templates/blox-spec.md`.
3. **Update AGENTS.md** — if you add a new crate, update the repository layout and "Where to Find Things" table. If you add or modify an invariant, update the invariants list.
4. **Update the building guide** — if changes affect how downstream users build bloxes, update `skills/building-with-bloxide/SKILL.md` and `reference.md`.

The spec must always be ahead of or equal to the code.
