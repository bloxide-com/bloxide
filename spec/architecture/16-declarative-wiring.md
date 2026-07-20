# Declarative Wiring & Handle Injection

## Problem Statement

Today, the wiring binary (e.g., `examples/tokio-demo.rs`) is hand-written Rust that knows:
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
    DemoBehavior::default(),
);
```

## Design

### The message handle lifecycle has three stages

1. **Declaration** (blox.toml) — "this blox needs a `peer_ref`, and it's a constructor param"
2. **Injection** (wiring manifest) — "ping's `peer_ref` comes from pong's channel"
3. **Acquisition** (runtime) — pool gets `worker_refs` by calling the spawn factory

Stage 1 is handled by the `role = "ctor"` field in `[[context.uses]]` (see spec 18). Stage 2 is the wiring manifest. Stage 3 is action functions.

### The wiring manifest

A separate TOML file (`system.toml` or `wiring.toml`) that describes the actor system topology:

```toml
# system.toml

[[actors]]
name = "timer"
blox = "bloxide-timer"
# Timer is a service — no constructor params needed from wiring

[[actors]]
name = "ping"
blox = "ping-blox"
behavior = "DemoBehavior"
behavior_traits = ["CountsRounds", "HasCurrentTimer"]

  [actors.inject]
  self_ref = { source = "self" }           # supervisor creates channel, injects self_ref
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
strategy = "one_for_one"
children = ["ping", "pong"]

  [supervision.policies]
  ping = { restart = { max = 1 } }
  pong = { stop = true }
```

### How handles are obtained

#### At spawn time (constructor params)

The supervisor is responsible for:
1. Creating the child's channel(s) → gets `self_ref` + mailbox
2. Knowing the child's message type from the blox spec
3. Passing `self_id` + `self_ref` to the constructor
4. Injecting cross-actor refs (`peer_ref`, `timer_ref`) from the wiring manifest

The wiring manifest tells the supervisor:
- `self_ref = { source = "self" }` → create a channel for this actor, inject the ref
- `peer_ref = { source = "actor", actor = "pong" }` → use pong's `self_ref` channel

For actors with multiple mailboxes (like worker: `WorkerMsg` + `WorkerCtrl`), the wiring manifest specifies which channel maps to which field:

```toml
[[actors]]
name = "worker-1"
blox = "worker-blox"
behavior = "WorkerBehavior"
behavior_traits = ["HasCurrentTask", "HasWorkerPeers"]

  [actors.inject]
  self_ref = { source = "self", mailbox = 0 }  # domain channel
  pool_ref = { source = "actor", actor = "pool" }
```

#### At runtime (dynamic discovery)

Some handles are obtained at runtime, not construction:
- `worker_refs` — pool discovers workers by calling the spawn factory
- `worker_ctrls` — same, the control channel ref

These are `role = "state"` fields — zero-initialized, populated by action functions. The wiring manifest doesn't inject them; the spawn factory provides them.

For dynamic spawning, the wiring manifest declares the spawn factory:

```toml
[[actors]]
name = "pool"
blox = "pool-blox"

  [actors.spawn_factory]
  crate = "tokio_pool_demo_impl"
  function = "spawn_worker_tokio"
  # The factory returns (domain_ref, ctrl_ref) which the pool stores
  # in worker_refs and worker_ctrls at runtime
```

### What the codegen produces from the wiring manifest

A wiring binary `main.rs` that:

1. **Creates channels** for each actor based on its mailboxes
2. **Constructs each context** with the right refs, looking up cross-actor refs from the wiring graph
3. **Constructs the behavior type** if the actor needs one (from `behavior` + `behavior_traits`)
4. **Wires the supervisor tree** — creates `ChildGroup`, adds children with policies
5. **Spawns everything** — calls the runtime's spawn function for each actor
6. **Starts the supervisor** — dispatches `LifecycleCommand::Start`

```rust
// Generated main.rs (sketch)
fn main() {
    // Create channels
    let timer_ref = bloxide_tokio::spawn_timer!(8);
    let ((ping_ref,), ping_mbox) = bloxide_tokio::channels! { PingPongMsg(16) };
    let ((pong_ref,), pong_mbox) = bloxide_tokio::channels! { PingPongMsg(16) };

    // Construct contexts
    let ping_ctx = PingCtx::new(
        ping_ref.id(),
        pong_ref.clone(),      // peer_ref from pong
        ping_ref.clone(),      // self_ref from own channel
        timer_ref.clone(),     // timer_ref from timer
        DemoBehavior::default(),
    );
    let pong_ctx = PongCtx::new(
        pong_ref.id(),
        ping_ref.clone(),      // peer_ref from ping
    );

    // Wire supervisor
    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
    bloxide_tokio::spawn_child!(group, ping_task(...), ChildPolicy::Restart { max: 1 });
    bloxide_tokio::spawn_child!(group, pong_task(...), ChildPolicy::Stop);
    // ... start supervisor
}
```

### Wiring for different runtimes

The wiring manifest is runtime-agnostic. The codegen produces runtime-specific binaries:
- **Tokio** — uses `bloxide_tokio::channels!`, `tokio::spawn`, `bloxide_tokio::spawn_child!`
- **Embassy** — uses `bloxide_embassy::channels!`, `embassy::spawn`
- **Test** — uses `TestRuntime::channel`, synchronous dispatch

The runtime is selected via a `runtime` field in the wiring manifest:

```toml
[system]
runtime = "tokio"  # or "embassy", "test"
```

### Relationship to blox.toml

Each blox.toml declares what constructor params it needs (via `role = "ctor"` fields). The wiring manifest declares what to inject into those params. The codegen matches them:

- blox.toml says `peer_ref` is a `ctor` field of type `ActorRef<PingPongMsg, R>`
- wiring.toml says `peer_ref = { source = "actor", actor = "pong" }`
- codegen emits `pong_ref.clone()` in the constructor call

Validation: the codegen checks that every `ctor` field in blox.toml has a corresponding `inject` entry in the wiring manifest, and that the types match (message type, runtime generic).

### Supervisor integration

The supervisor already handles child registration and lifecycle. The wiring manifest extends this:

1. **Static children** — declared in `[[supervision]]` with policies. The supervisor starts them on `Start`.
2. **Dynamic children** — spawned at runtime via the spawn factory. The supervisor registers them dynamically (already supported via `SupervisorControl::RegisterChild`).
3. **Health checks** — optional `health_check_interval_ms` in the wiring manifest.

### Visual Editor Integration

The wiring manifest drives a visual editor where you:
- Drag blox instances onto a canvas
- Draw connections between actors (message type flows from A to B)
- The editor infers `inject` entries from the connections
- Set supervision policies (restart / stop) per child
- Set spawn factories for dynamic actors
- Pick the runtime (Tokio / Embassy / Test)

The codegen produces the complete binary. The only hand-written Rust is action function bodies and guard predicates in actions crates.
