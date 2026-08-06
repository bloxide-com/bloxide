# 12 — Context Crate Pattern

> **When would I use this?** Use this document when organizing domain code,
> understanding the four-layer application structure (messages, context, blox, binary),
> or learning how context crates and impl crates keep bloxes portable.

> **Renamed (Phase 1-3, July 2026):** This document was formerly "Action-Crate
> Pattern." Action crates have been eliminated; action functions now live in
> context crates. The five-layer structure is now four layers.

## Overview

Bloxide applications follow a four-layer structure that keeps runtime details out of
domain actors:

1. Messages
2. Context (includes actions)
3. Blox
4. Binary / wiring

The goal is simple: blox crates stay declarative and runtime-agnostic with zero logic,
while context crates carry the traits and action functions they reference.

```
Before:  messages → actions → context → impl → blox → (codegen) → binary
After:   messages → context (includes actions) → blox → binary
         impl = optional reusable behavior libraries
```

## The Four Layers

```mermaid
flowchart TD
    Messages["Layer 1: messages
    shared plain-data enums/structs"]
    Context["Layer 2: context
    traits + free action functions
    taking concrete params"]
    Blox["Layer 3: blox
    purely declarative topology
    blox.toml + generated stubs"]
    Binary["Layer 4: binary/wiring
    system.toml + two-stage codegen
    + optional impl crates"]

    Messages --> Context
    Context --> Blox
    Blox --> Binary
    Context --> Binary
```

### Layer 1 — Messages

Message crates contain plain data only. When two or more bloxes share a protocol,
the shared enum lives in a dedicated `*-messages` crate.

```rust
pub enum PingPongMsg {
    Ping(Ping),
    Pong(Pong),
    Resume(Resume),
}
```

Rules:

- No runtime types in messages.
- Prefer named struct variants such as `Ping(Ping { round })`.
- Keep shared protocols in dedicated message crates to avoid circular deps.

### Layer 2 — Context

Context crates are the portable interface layer. They define:

- **Free action functions** taking concrete params (e.g., `increment_round(&mut u32)`,
  `send_ping::<R>(ActorId, &ActorRef<M, R>, u32)`), each returning `ActionResult`

There is no `B` generic, no `#[delegatable]`, no `#[delegates]`, no accessor traits.
State is stored as plain fields on the context struct; action functions take the
fields they need as concrete parameters.

```rust
// crates/blox-ctx-rounds/src/lib.rs
pub fn increment_round(round: &mut u32) -> ActionResult {
    *round += 1;
    ActionResult::Ok
}
```

```rust
// crates/blox-ctx-ping-pong/src/lib.rs
pub fn send_ping<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    round: u32,
) -> ActionResult {
    ActionResult::from(peer_ref.try_send(self_id, PingPongMsg::Ping(Ping { round })))
}
```

Rules:

- Context crates may contain portable generic logic.
- They must not import Embassy, Tokio, file I/O, or executor-specific code.
- No `bloxide-log` dependency (logging ripped out of blox crates; context crates may use it if needed).

### Layer 3 — Blox

Blox crates are purely declarative. They contain:

- `blox.toml` — declares topology, event matching, action names, field mappings, guard expressions
- Generated `ctx.rs` — plain struct with fields, no `B`, no `#[delegates]`
- Generated `topology.rs` — state enum, event enum
- Generated `spec_skeleton.rs` — **stub actions** (no-op closures) + **real guards** (direct field comparisons) + real event matching + real state topology
- `lib.rs` — re-exports, constants (like `MAX_ROUNDS`), no logic
- No `actions.rs`
- No `bloxide-log` dependency
- Compiles standalone without any impl crate

They depend on `bloxide-core`, message crates, and context crates, but never on a
runtime crate or an impl crate.

```rust
// Generated ctx.rs — plain fields, no B generic, no accessor traits
pub struct PingCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    pub current_timer: Option<TimerId>,
    pub round: u32,
}
```

### Layer 4 — Binary / Wiring

The wiring binary is the only layer that knows:

- which runtime is being used
- which impl crates provide impl-specific functions (if any)
- which channels and capacities should be created
- how actors are spawned and connected

It creates channels, constructs contexts, and starts tasks. The system codegen
reads `system.toml` and generates concrete `spec_skeleton.rs` with real action
closures inlined from context/impl crates.

## Two-Stage Codegen

Both stages run under `cargo blox generate` (which lints first and is
idempotent); all generated artifacts (the whole `target/bloxide-generated/`
tree — blox crates and example crates) are gitignored.

### Stage 1 — Blox-level

From `blox.toml`, generates stub action closures with real guards. The blox
compiles standalone without any impl dependency. Guard/transition logic is
functional with stubs; only side-effecting actions are stubs. A stub is a
marker plus a no-op `ActionResult::Ok`:

```rust
// target/bloxide-generated/crates/ping-blox/src/generated/spec_skeleton.rs — stub actions
|_ctx, _ev| {
    let _stub = "forward_ping";
    ActionResult::Ok
}
```

### Stage 2 — System-level

From `system.toml`, resolves impl crates and generates the app's `main.rs`
plus concrete `spec_skeleton.rs` files with real action closures calling
context/impl crate functions. Guards are unchanged from blox-level (already
real).

Transition action closures return the action function's `ActionResult`
verbatim (the uniform contract — the guard can react to failure via
`results.any_failed()`):

```rust
// examples/<app> → target/bloxide-generated/examples/<app>/src/generated/ping_spec_skeleton.rs — concrete, impl inlined
|ctx, _ev| blox_ctx_ping_pong::send_ping(ctx.self_id, &ctx.peer_ref, ctx.round)
```

Entry/exit actions are infallible (`fn(&mut Ctx)`), so their closures call
the function and discard its `ActionResult`:

```rust
|ctx| {
    blox_ctx_ping_pong::send_initial_ping(ctx.self_id, &ctx.peer_ref, &mut ctx.round);
}
```

### `system.toml` example

```toml
[[actors]]
name = "ping"
blox = "ping-blox"
# No impl crate needed — all actions come from context crates

[[actors]]
name = "pool"
blox = "pool-blox"
impl_crate = "tokio_pool_demo_impl"  # Provides process_work and spawn functions
```

### Action import resolution

The system codegen resolves action functions by convention:
- Actions with `impl_required = true` → `<impl_crate>::<fn_name>`
- Actions with `impl_required = false` → `<context_crate>::<fn_name>`
  (resolved from the `[[context.actions]]` `crate` field)

## Impl Crates (Optional)

Impl crates are optional libraries of impl-specific functions. No `B` struct,
no trait impls — just free functions taking concrete params.

Use an impl crate when a blox needs behavior that is:
- runtime- or platform-specific (e.g., spawning workers on Tokio)
- deployment-specific (e.g., different work processing logic)

Bloxes with no impl-specific behavior (like Ping, Pong, Counter, BHSM) need no
impl crate. The system codegen generates concrete code using only context crate
functions.

```rust
// crates/impl/tokio-pool-demo-impl/src/lib.rs
pub fn process_work(task_id: &mut u32, result: &mut u32, do_work: &DoWork) -> ActionResult {
    *task_id = do_work.task_id;
    *result = do_work.task_id * 2;
    ActionResult::Ok
}
```

Dependency direction stays one-way: the wiring binary depends on both the blox crate
and the impl crate; the blox crate never depends on the impl crate.

## Field Declaration Convention

Context fields are plain fields on the context struct. The codegen auto-emits
`self_id` as the first field. Constructor parameters come from `[[context.uses]]`
entries with `role = "ctor"` (injected at wiring time); `role` is validated —
only `ctor` and `state` are accepted. The `crate` key on a `[[context.uses]]`
entry is optional and informational (used by scaffolding and visualization, not
by codegen). State fields come from `[[context.fields]]` entries (or
`[[context.uses]]` with `role = "state"`) and are zero-initialized via
`Default::default()` in `Ctx::new()`.

| Declaration | Use Case | Example |
|---------------|----------|---------|
| `self_id: ActorId` | Auto-emitted first field | always present |
| `[[context.uses]] role = "ctor"` | Constructor param (injected at wiring) | `peer_ref`, `timer_ref`, `spawn_fn` |
| `[[context.fields]]` | State field | `round`, `pending` |
| `[[context.uses]] role = "state"` | State field from a composable crate | `spawn_queue` |

## Supervisor As The Same Pattern

The supervisor follows the same layering model:

- `bloxide-child-management` provides action functions taking concrete params
- `SupervisorSpec<R>` is the reusable `MachineSpec`
- the wiring layer builds a `ChildGroup<R>` and injects it into `SupervisorCtx<R>`

That is why supervision is reusable without requiring a custom per-project
supervisor actor implementation.

## Related Docs

- [05-actions.md](05-actions.md) for action function mechanics and two-stage codegen
- [07-supervision.md](07-supervision.md) for the reusable supervisor model
- [08-application.md](08-application.md) for end-to-end wiring
- [10-dynamic-actors.md](10-dynamic-actors.md) for runtime spawning patterns
- [13-composable-context-crates.md](13-composable-context-crates.md) for context crate composition
