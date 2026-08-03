# Application Wiring

An application wires bloxes together: it creates channels, builds contexts,
constructs state machines, and spawns actor tasks. In the current architecture
this wiring is **generated**, not hand-written: `system.toml` is the source of
truth and `cargo blox generate` (system-level codegen) emits `src/main.rs` and
the concrete specs. See
[16-declarative-wiring.md](16-declarative-wiring.md) and
[17-blox-toml-source-of-truth.md](17-blox-toml-source-of-truth.md).

The canonical examples are the demo apps: `apps/tokio-demo/`,
`apps/tokio-minimal-demo/`, `apps/tokio-pool-demo/`, `apps/embassy-demo/` —
each has a `system.toml` plus generated `src/main.rs`.

## system.toml Drives Everything

```toml
# apps/tokio-demo/system.toml (abridged)
[system]
runtime = "tokio"
name = "tokio-demo"

[[actors]]
name = "timer"
blox = "bloxide-timer"
kind = "timer"

[[actors]]
name = "ping"
blox = "ping-blox"

  [actors.inject]
  self_ref = { source = "self" }
  peer_ref = { source = "actor", actor = "pong" }
  timer_ref = { source = "actor", actor = "timer" }

[[supervision]]
supervisor = "bloxide-supervisor"
strategy = "when_any_done"
children = ["ping", "pong"]

  [supervision.policies]
  ping = { stop = true }
  pong = { stop = true }
```

From this, the codegen emits: typed channels per actor, `Ctx::new(...)` calls
with refs injected per `[actors.inject]`, a `ChildGroupBuilder` with
`spawn_child!` per supervised child, and the supervisor boot sequence.

## Lifecycle Is Dispatch-Driven

Actors never call `machine.start()` / `machine.reset()` — lifecycle commands
flow through `dispatch()` and are intercepted at the VirtualRoot level
(see [02-hsm-engine.md](02-hsm-engine.md)). This includes the
supervisor itself at boot:

```rust
// From apps/tokio-demo/src/main.rs (generated)
let mut sup_machine = ::bloxide_core::StateMachine::<
    crate::generated::bloxide_supervisor_spec_skeleton::SupervisorSpec<TokioRuntime>,
>::new(sup_ctx);
sup_machine.dispatch(
    ::bloxide_supervisor::SupervisorEvent::<TokioRuntime>::Lifecycle(LifecycleCommand::Start),
);
supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;
```

`Start` enters `SupervisorState::Running`, whose `on_entry` calls
`start_children` — sending `Start` to each child's lifecycle channel. The
runtime observes `DispatchOutcome` after every dispatch in `run()` and reports
`ChildLifecycleEvent` back to the supervisor automatically.

## Boot Sequence (generated main.rs)

1. `spawn_timer!(capacity)` — spawn the timer service, get `timer_ref`
2. `channels! { MsgType(cap), ... }` per domain actor → `(refs, mailboxes)`
3. `Ctx::new(self_id, ...refs)` per actor — refs injected, internal state defaulted
4. `StateMachine::new(ctx)` per actor — construction is silent (in implicit Init)
5. `ChildGroupBuilder::new(GroupShutdown::...)`; `spawn_child!(group, task(machine, mbox, id), ChildPolicy::...)` per supervised child — creates the per-child lifecycle channel and registers the child
6. `group.finish()` → `(ChildGroup, sup_notify_rx, sup_control_rx)`
7. `SupervisorCtx::new(sup_id, children, sup_notify_ref)` → supervisor machine
8. `sup_machine.dispatch(Lifecycle(Start))` → `Running::on_entry` starts all children
9. `root_task!` / task `.await` — the root supervisor runs until the program is done

## Wiring Macros (per runtime)

Provided by `bloxide-tokio` / `bloxide-embassy` (same names, runtime-specific
implementations):

- `channels! { Msg(cap), ... }` — create typed domain channels, return `(refs, mailboxes)`
- `next_actor_id!()` — allocate a compile-time actor ID
- `actor_task!(name, Spec)` / `actor_task_supervised!(name, Spec)` — actor task wrappers around `run()` with `RunConfig::unsupervised()` / `RunConfig::supervised(...)`
- `root_task!(name, Spec)` — root actor wrapper around `run()` with `RunConfig::root()`
- `spawn_child!(group, task(...), policy)` — register + spawn a supervised child
- `spawn_timer!(capacity)` — spawn the timer service task

## Rules

- All static wiring happens before the executor starts (Embassy) or in `main` before awaiting the root task (Tokio). Dynamic actor creation at runtime is a Tokio/TestRuntime capability — see [11-dynamic-actors.md](11-dynamic-actors.md).
- Never pass an `ActorRef` through a message; all refs are injected via `Ctx::new()` at wiring time.
- Domain `Mailboxes` tuples contain **no lifecycle stream** — lifecycle channels are created by `spawn_child!` and are invisible to blox code.
- Internal state fields (counters, round numbers) are zero-initialized via
  `Default::default()` in the generated `Ctx::new()`, never at the wiring site.
- Actor IDs come from two disjoint spaces: static wiring uses the compile-time
  proc-macro counter (`channels!`, `next_actor_id!`, `spawn_timer!`) starting
  at 1; dynamically spawned actors use `DynamicChannelCap::alloc_actor_id`,
  whose counter starts at `DYNAMIC_ACTOR_ID_BASE` (1_000_000) — see
  [11-dynamic-actors.md](11-dynamic-actors.md).
- The supervisor is started via `dispatch()` of `LifecycleCommand::Start`, like every other lifecycle transition.
