# Declarative Wiring & Handle Injection

## Problem Statement

Today, the wiring binary (e.g., `apps/tokio-demo/src/main.rs`) is hand-written Rust that knows:
1. Which channels to create
2. Which actors to construct
3. Which `ActorRef`s to pass to which constructor
4. How to wire the supervisor tree
5. How to spawn everything

This is the last piece of hand-written Rust in the blox pipeline. If the goal is "everything visual in the UI except action function bodies," the wiring step must be declarative.

The user should not be writing:
```rust
let ping_ctx = PingCtx::new(
    ping_id,
    pong_ref.clone(),    // peer_ref — how do I know to pass pong's ref?
    ping_ref.clone(),    // self_ref — how do I know this is my own ref?
    timer_ref,           // timer_ref — how do I know to pass the timer's ref?
);
```

## Design

### The message handle lifecycle has three stages

1. **Declaration** (blox.toml) — "this blox needs a `peer_ref`, and it's a constructor param"
2. **Injection** (wiring manifest) — "ping's `peer_ref` comes from pong's channel"
3. **Acquisition** (runtime) — the pool learns `worker_refs` from `SpawnedWorker` replies

Stage 1 is handled by the `role = "ctor"` field in `[[context.uses]]` (see spec 13). Stage 2 is the wiring manifest. Stage 3 is action functions.

### The wiring manifest

A separate TOML file (`system.toml`) that describes the actor system topology:

```toml
# system.toml

[system]
runtime = "tokio"

[[actors]]
name = "timer"
blox = "bloxide-timer"
kind = "timer"   # timer service — no channels/task/context in main.rs

[[actors]]
name = "ping"
blox = "ping-blox"

  [actors.inject]
  self_ref = { source = "self" }           # wiring creates channel, injects self_ref
  peer_ref = { source = "actor", actor = "pong" }  # pong's channel ref
  timer_ref = { source = "actor", actor = "timer" } # timer's channel ref

[[actors]]
name = "pong"
blox = "pong-blox"

  [actors.inject]
  self_ref = { source = "self" }
  peer_ref = { source = "actor", actor = "ping" }  # ping's channel ref

[[supervision]]
supervisor = "bloxide-supervisor"
strategy = "when_any_done"   # or "when_all_done" — maps to GroupShutdown::WhenAnyDone / WhenAllDone
children = ["ping", "pong"]

  [supervision.policies]
  ping = { stop = true }
  pong = { stop = true }
```

### How handles are obtained

#### At spawn time (constructor params)

The generated wiring is responsible for:
1. Creating the child's channel(s) → gets `self_ref` + mailbox
2. Knowing the child's message type from the blox spec
3. Passing `self_id` + `self_ref` to the constructor
4. Injecting cross-actor refs (`peer_ref`, `timer_ref`) from the wiring manifest

The wiring manifest tells the codegen:
- `self_ref = { source = "self" }` → create a channel for this actor, inject the ref
- `peer_ref = { source = "actor", actor = "pong" }` → use pong's `self_ref` channel

For actors with multiple mailboxes (like the pool: `PoolMsg` + `SpawnedWorker<...>` replies), the wiring manifest binds each secondary channel to its inject field with `source = "self_secondary"` and an `index` (the channel's position in the generated `channels!` call, default 1):

```toml
[[actors]]
name = "pool"
blox = "pool-blox"

  [actors.inject]
  self_ref = { source = "self" }                               # primary channel
  spawn_reply_ref = { source = "self_secondary", index = 1 }   # second channel
  spawn_ref = { source = "actor", actor = "supervisor", field = "control" }
```

Every secondary mailbox declared in the blox's `[event]` section must have a matching
`self_secondary` inject entry at its index — a missing entry is a hard codegen error.

#### At runtime (dynamic discovery)

Some handles are obtained at runtime, not construction:
- `worker_refs` — the pool learns worker refs from the `SpawnedWorker` reply sent back
  by the spawn function
- `worker_ctrls` — same, the control channel ref

These are `role = "state"` fields — zero-initialized, populated by action functions. The
wiring manifest doesn't inject them.

For dynamic spawning, the wiring manifest injects the spawn function itself as a
constructor param with `source = "factory"`, and declares the dynamically spawned actor
with `kind = "dynamic"` (no channels/task in `main.rs` — the impl crate's factory
builds it at runtime):

```toml
[[actors]]
name = "pool"
blox = "pool-blox"
impl_crate = "tokio_pool_demo_impl"
features = ["dynamic"]

  [actors.inject]
  spawn_fn = { source = "factory", crate = "tokio_pool_demo_impl", function = "build_worker" }
  # The factory builds the worker's ActorParts (pure construction); the codegen
  # composes it with bloxide_spawn::spawn_actor_task, which spawns the task and
  # returns SpawnOutput for the supervisor. The factory also sends a
  # SpawnedWorker reply (domain_ref, ctrl_ref) which the pool stores
  # in worker_refs and worker_ctrls at runtime

[[actors]]
name = "worker"
blox = "worker-blox"
impl_crate = "tokio_pool_demo_impl"
kind = "dynamic"
```

When the factory crate is a dynamic actor's `impl_crate` (the usual case), the codegen
emits a monomorphizing closure that fills in the system-level concrete spec and
composes the domain build with the platform spawn
(`(|req, notify| ::bloxide_spawn::spawn_actor_task(::tokio_pool_demo_impl::build_worker::<WorkerSpec<TokioRuntime>>(req), notify)) as _`);
for a plain function that assembles `SpawnOutput` directly it emits a path expression
with a cast (`::my_impl_crate::my_factory as _`). See
`crates/tools/bloxide-codegen/src/system_wiring.rs` and the real example in
`apps/tokio-pool-demo/system.toml`. The codegen also adds a `bloxide-spawn` dependency
to the generated app's `Cargo.toml` when a factory injection exists (the emitted
closure names `::bloxide_spawn::spawn_actor_task`).

### What the codegen produces from the wiring manifest

A wiring binary `main.rs` that:

1. **Creates channels** for each actor based on its mailboxes
2. **Constructs each context** with the right refs, looking up cross-actor refs from the wiring graph
3. **Wires the supervisor tree** — creates `ChildGroup`, adds children with policies
4. **Spawns everything** — calls the runtime's spawn function for each actor
5. **Starts the supervisor** — dispatches `LifecycleCommand::Start`

```rust
// Generated main.rs (sketch — see apps/tokio-pool-demo/src/main.rs for real output)
#[tokio::main]
async fn main() {
    // Create channels
    let timer_ref = bloxide_tokio::spawn_timer!(8);
    let ((ping_ref,), ping_mbox) = bloxide_tokio::channels! { PingPongMsg(16), };
    let ((pong_ref,), pong_mbox) = bloxide_tokio::channels! { PingPongMsg(16), };
    let ping_id = ping_ref.id();
    let pong_id = pong_ref.id();

    // Construct contexts (plain fields — no behavior type)
    let ping_ctx = PingCtx::new(
        ping_id,
        pong_ref.clone(),      // peer_ref from pong
        ping_ref.clone(),      // self_ref from own channel
        timer_ref.clone(),     // timer_ref from timer
    );
    let pong_ctx = PongCtx::new(
        pong_id,
        ping_ref.clone(),      // peer_ref from ping
    );
    let ping_machine = bloxide_core::StateMachine::new(ping_ctx);
    let pong_machine = bloxide_core::StateMachine::new(pong_ctx);

    // Wire supervisor
    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone, 2);
    bloxide_tokio::spawn_static_child!(group, ping_task(ping_machine, ping_mbox, ping_id), ChildPolicy::Stop);
    bloxide_tokio::spawn_static_child!(group, pong_task(pong_machine, pong_mbox, pong_id), ChildPolicy::Stop);
    // ... finish group, construct SupervisorCtx, start supervisor
}
```

### Codegen internals: the ref symbol table and the two-phase supervisor split

> This section was moved here from spec 16 (§10), which now only points here.

The `source = "actor"` form injects another actor's named ref. The optional `field`
parameter generalizes this to any named ref an actor exposes — not just the primary
channel. `field` defaults to `"primary"` (the actor's primary channel ref).

The codegen maintains a **symbol table** — a registry mapping `(actor_name, field_name)`
to Rust variable idents. Each generation phase registers the symbols it creates:

- Channel creation: registers `(actor, "primary")` → `{actor}_ref` for each actor
- Supervisor setup: registers `(supervisor, "control")` → the extracted `control_ref`
  from `ChildGroupBuilder`, `(supervisor, "notify")` → the extracted `notify_ref`

The injection handler looks up `(actor, field)` in the symbol table:

```rust
// In system_wiring.rs context construction (simplified)
} else if source.source == "actor" {
    let field_selector = source.field.as_deref().unwrap_or("primary");
    if field_selector == "primary" {
        ctor_args.push(quote! { #primary_ref_ident.clone() });
    } else {
        // Named ref: look up in symbol table (e.g. supervisor's
        // "control" or "notify" refs). Unknown names are hard errors.
        let sym = symbol_table.get(&(src_actor.to_string(), field_selector.to_string()))
            .ok_or_else(|| anyhow!("actor '{}' has no ref '{}'", src_actor, field_selector))?;
        ctor_args.push(quote! { #ref_ident.clone() });
    }
}
```

This is general because:

- **Any blox can inject any other blox's named refs** — not just channel refs, not just
  supervisor refs. A user's custom job dispatcher blox exposes its own `control` and
  `notify` mailboxes; any spawning blox injects them the same way.
- **Adding a new named ref to any blox** just means registering it in the symbol table —
  no new source type, no codegen change.
- **Multiple managing bloxes** are handled by the actor name (each has a unique name in
  `system.toml`).

The supervisor's `control_ref` and `notify_ref` must exist **before** context
construction so other actors (e.g. the pool) can inject them, but children can only be
added **after** the machines exist. So `ChildGroupBuilder` is used in two phases:

- **Phase 1** (before context construction): create the builder, extract
  `control_ref()` / `notify_ref()`, register them in the symbol table.
- **Phase 2** (after machine construction): add children via `spawn_static_child!`, call
  `finish()`, construct `SupervisorCtx`.

The generated `main` body is ordered accordingly:

```rust
// Generated main (simplified — real output: apps/tokio-pool-demo/src/main.rs)
#(#channel_stmts)*              // 1. Create channels for all actors
#(#supervisor_setup_stmts)*     // 2. Builder + control_ref + notify_ref (symbol table)
#(#ctx_stmts)*                  // 3. PoolCtx injects supervisor refs from symbol table
#(#machine_stmts)*              // 4. Machines constructed
#(#supervisor_finish_stmts)*    // 5. spawn_static_child! + finish() + SupervisorCtx
#(#bootstrap_send_stmts)*       // 6. Bootstrap messages
#(#supervisor_run_stmts)*       // 7. Spawn supervisor + actor tasks
```

The `ChildGroupBuilder` is a single `let mut group` that spans both phases. Phase 1
extracts refs; phase 2 adds children and consumes the builder.

### Wiring for different runtimes

The wiring manifest is runtime-agnostic. The codegen produces runtime-specific binaries:
- **Tokio** — uses `bloxide_tokio::channels!`, `tokio::spawn`, `bloxide_tokio::spawn_static_child!`
- **Embassy** — uses `bloxide_embassy::channels!`, `embassy::spawn`

The runtime is selected via a `runtime` field in the wiring manifest:

```toml
[system]
runtime = "tokio"  # or "embassy"
```

These are the only two supported values — the codegen
(`system_wiring.rs::generate`) bails on anything else, including `"test"`. Tests drive
`TestRuntime` directly in Rust (see `runtimes/bloxide-test-runtime`), not through the
wiring manifest.

### Relationship to blox.toml

Each blox.toml declares what constructor params it needs (via `role = "ctor"` fields). The wiring manifest declares what to inject into those params. The codegen matches them:

- blox.toml says `peer_ref` is a `ctor` field of type `ActorRef<PingPongMsg, R>`
- wiring.toml says `peer_ref = { source = "actor", actor = "pong" }`
- codegen emits `pong_ref.clone()` in the constructor call

Validation: the codegen checks that every `ctor` field in blox.toml has a corresponding
`inject` entry in the wiring manifest (coverage), and that every inject entry names a
real constructor field. This is **name and coverage checking only — no type checking**;
a type mismatch (message type, runtime generic) fails later at `cargo build` of the
generated binary. An inject entry naming a field that does not exist at all is a **hard
error** (typically a typo) — the only exception is a field that exists in `blox.toml`
but is feature-gated off in the current build, which is tolerated (the injection is
cfg'd out together with the field).

### Supervisor integration

The supervisor already handles child registration and lifecycle. The wiring manifest extends this:

1. **Static children** — declared in `[[supervision]]` with policies. The supervisor starts them on `Start`.
2. **Dynamic children** — spawned at runtime via the injected spawn function (`source = "factory"`). The supervisor registers them dynamically via `ChildCtrl::RegisterDynamicChild` (from `bloxide-child-management::control`, sent by the `spawn_dynamic_child` helper in `bloxide-spawn`).

### Visual Editor Integration

The wiring manifest drives a visual editor where you:
- Drag blox instances onto a canvas
- Draw connections between actors (message type flows from A to B)
- The editor infers `inject` entries from the connections
- Set supervision policies (reset / stop) per child
- Set factory injections (`source = "factory"`) for dynamic actors
- Pick the runtime (Tokio / Embassy)

The codegen produces the complete binary. The only hand-written Rust is action function bodies and guard predicates in context crates.
