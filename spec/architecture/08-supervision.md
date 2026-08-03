# Supervision

> **When would I use this?** Use this document when setting up supervision,
> understanding the four-level lifecycle model (`reset → stop → abort → kill`),
> KillCapability (the kill ripcord for unresponsive actors), or learning how child lifecycle
> events flow to supervisors. For lifecycle command handling details, see `02-hsm-engine.md`
> → "Four-Level Lifecycle" and "Lifecycle Command Handling at VirtualRoot".

The supervision model is inspired by Elixir/OTP. A **supervisor** is itself a state machine actor that monitors child actors and either restarts or permanently stops them in response to lifecycle triggers. Unlike OTP, the supervisor is a **generic library component** — users configure children and policies in the wiring layer without writing a custom blox.

Lifecycle control is handled entirely by the **runtime** — actors never see lifecycle commands and never hold a reference to their supervisor.

## Where the Pieces Live

Supervision is split across a core engine layer, a platform child-management layer, a spawn layer, and the supervisor topology itself:

| Type / Function | Crate |
|---|---|
| `LifecycleCommand`, `ChildLifecycleEvent`, `AbortCommand` | `bloxide-core::lifecycle` |
| `run`, `RunConfig` (the unified run loop) | `bloxide-core::runloop` |
| `report_outcome` (`DispatchOutcome` → `ChildLifecycleEvent`) | `bloxide-core::supervision` |
| `KillCapability`, `NoKill` | `bloxide-core::capability` |
| `ChildGroup`, `ChildPolicy`, `GroupShutdown`, `ChildAction`, `ChildPhase` | `bloxide-child-management` |
| Action fns (`start_children`, `stop_all_children`, `handle_done_or_failed`, `record_*`, `deregister_done`, `register_child`, `handle_register_dynamic_child`, `handle_health_check`) | `bloxide-child-management::actions` |
| `ChildCtrl`, `RegisterChild`, `RegisterDynamicChild` | `bloxide-child-management::control` |
| `ChildGroupBuilder<R, Ctrl>` (group channels via `GroupChannelCap`) | `bloxide-child-management::builder` |
| `SpawnCap`, `Kill`, `SpawnFn`, `SpawnOutput`, `ChildRegistrar`, `ChildCtrlRegistrar`, `spawn_child` | `bloxide-spawn` |
| Supervisor topology: `blox.toml`, generated code, `concrete_spec.rs` (test fixture), tests | `bloxide-supervisor` |

`bloxide-supervisor` owns **only** the supervisor state machine: its `blox.toml` topology, the generated `SupervisorSpec`/`SupervisorCtx`/`SupervisorEvent`/`SupervisorState`, an in-crate `concrete_spec.rs` used by its tests, and the tests themselves. All policy/shutdown logic lives in `bloxide-child-management` (a reusable platform primitive — any managing blox can use `ChildGroup`, not just the supervisor), and the spawn-registration bridge lives in `bloxide-spawn`.

## Core Principle: Actor Lifecycle Commands

Bloxide has a **four-level lifecycle model** (`reset → stop → abort → kill`), ordered from gentlest to most forceful. Two of the four levels are `LifecycleCommand` variants handled through the normal dispatch pipeline (`Reset`, `Stop`); the cooperative level (`Abort`) is delivered on a dedicated abort mailbox, and the final forceful level (`Kill`) is a runtime capability that bypasses dispatch entirely.

| Command / Capability | Target State | Through dispatch? | Callbacks | Can Restart? |
|---------|--------------|-------------------|-----------|--------------|
| `Reset` | User-defined initial operational state (`initial_state()`) | Yes | Full exit chain + entry chain for `initial_state()` (no `on_init_entry`) | Yes (already running) |
| `Stop` | Init | Yes | Full exit chain + `on_init_entry` | Yes (send `Start` to resume) |
| `Abort` (`AbortCommand`) | Task ends cooperatively | No (run loop breaks) | None | Yes (respawn the task) |
| `Kill` (`KillCapability::kill`) | Destroyed (task aborted in place) | No (runtime ripcord) | None | No (permanently dead) |

> For the full engine-level treatment of the four-level model (including how `Decision::Reset`/`Decision::Stop`/`Decision::Done` and the `DispatchOutcome` variants map to these levels), see `02-hsm-engine.md` → "Four-Level Lifecycle".

### Reset — Immediate Restart

`Reset` sends the actor through its exit chain (all `on_exit` callbacks fire), then enters the **user-defined initial operational state** (defined by `MachineSpec::initial_state()`). **Reset skips Init entirely** — no `on_init_entry` or `on_init_exit` fires. The `on_entry` callbacks for `initial_state()` are responsible for resetting domain state. The actor is immediately running again — no separate `Start` command is needed. The runtime reports `DispatchOutcome::Started(initial_state)`, which the supervisor sees as `ChildLifecycleEvent::Started`.

Use for: restart cycles where the actor should continue operating.

### Stop — Graceful Shutdown

`Stop` sends the actor through its exit chain (all `on_exit` callbacks fire), calls `on_init_entry` (for resource cleanup), and leaves the actor in **Init**. The task stays alive but suspended. Send `Start` to resume operation from `initial_state()`.

Use for:
- Graceful shutdown (callbacks run, clean exit)
- Pausing an actor with intent to resume later
- Dynamic actors you may want to restart

### Abort — Cooperative Self-Termination

`Abort` is sent as an `AbortCommand::Abort { child_id }` on a dedicated **abort mailbox** (separate from the lifecycle mailbox). The actor's run loop polls it alongside lifecycle and domain mailboxes; when an `AbortCommand` is received, the run loop breaks and the task ends cooperatively. No `dispatch()` is called, no exit callbacks fire, no `on_init_entry` fires.

The run loop synthesizes `DispatchOutcome::Aborted` itself (Abort bypasses the dispatch pipeline) and reports `ChildLifecycleEvent::Aborted` to the supervisor. The task is ended but was not externally destroyed — restarting requires respawning a new task. `ChildPolicy::Abort` sends `AbortCommand` on the child's `abort_ref`; the supervisor records the child as `Aborted` via `record_aborted()`.

Use for:
- Supervisor-initiated shutdown where you want the task to end but `Stop` does not fit (e.g. the actor is already in `Init`)
- Cases where you want the task gone through a cooperative, observable path (`Aborted` is reported; the task ends itself)

### Kill — Permanent Termination (Ripcord)

`KillCapability::kill(handle)` is the external ripcord. `ChildPolicy::Kill` calls `R::Kill::kill(handle)` — on dynamic runtimes the `Kill` impl (in `bloxide-spawn`) forwards to `SpawnCap::kill(handle)`, which destroys the task in place (for Tokio, `KillHandle = tokio::task::AbortHandle`). It works even on stuck/deadlocked actors that aren't polling any mailbox. No callbacks, no dispatch, no mailbox. The task is permanently dead; its ID will never be valid again.

Because the kill is synchronous and bypasses the child's run loop, the child never reports an outcome. Instead, **`ChildGroup` synthesizes `ChildLifecycleEvent::Killed { child_id }` directly onto the supervisor's notify channel** when it applies `ChildPolicy::Kill` (this is why `ChildGroup::handle_done_or_failed` takes the notify ref). The supervisor handles it like any other lifecycle event, recording the child via `record_killed` (phase → `Killed`). There is no `DispatchOutcome::Killed` — the `Killed` event never passes through the child's run loop.

**Kill has two purposes:**

1. **Unresponsive actors** — stuck in infinite loops, deadlocks, or blocking calls; cannot process `Stop`, `Reset`, or `Abort`. Kill forces termination when cooperation is not possible. (When the actor *can* still yield to its run loop, prefer `Abort`.)

2. **Cleanup** — freeing resources for an actor that has already been stopped or aborted, when you want the task handle gone immediately.

**For dynamic actors**, the cooperative shutdown ladder is:
```
Stop → actor goes to Init (suspended, callbacks ran)
  ├─ Start → actor resumes operation from initial_state()
  └─ Abort → task ends cooperatively (ChildLifecycleEvent::Aborted)
                └─ Kill → if Abort is not serviced in time, ripcord the task
```

**For static actors**, `Kill`/`Abort` policies are not available at all: `ChildGroup::add` **panics** if either is requested, because static children have no abort mailbox or kill handle. Static children use `Reset`/`Stop` policies only.

**Actors have zero knowledge of their supervisor.** No `supervisor_ref` in context, no lifecycle messages in event enums, no root rules for Reset/Stop/Ping.

## KillCapability: The Kill Ripcord

`KillCapability` is the **runtime capability** behind the `Kill` level of the four-level lifecycle model. It terminates an actor's task immediately, bypassing the normal dispatch lifecycle. It is used for unresponsive actors that cannot service `Stop`/`Reset`/`Abort`, or for cleanup when the task handle must be freed immediately.

In the four-level model, `KillCapability` is **only** invoked by `ChildPolicy::Kill`. The cooperative `Abort` path uses `AbortCommand` on the abort mailbox instead (see [Abort — Cooperative Self-Termination](#abort--cooperative-self-termination) above).

### KillCapability vs. Lifecycle Commands and Abort

| Command/Cap | Path | Callbacks | When Used |
|---|---|---|---|
| `Reset` | `handle_lifecycle(Reset)` → exit + entry chain for `initial_state()` | `on_exit` (all states), `on_entry` for `initial_state()` (no `on_init_entry`) | Restart cycle |
| `Stop` | `handle_lifecycle(Stop)` → exit chain → Init | `on_exit` (all states), `on_init_entry` | Clean shutdown, suspend |
| `Abort` (`AbortCommand`) | abort mailbox → run loop breaks (no dispatch) | **None** | Cooperative task termination |
| `KillCapability::kill` | Runtime task kill (bypasses dispatch and mailboxes) | **None** | Unresponsive actors, resource cleanup (ripcord) |

### KillCapability Trait Definition

```rust
// In bloxide-core/src/capability.rs
pub trait KillCapability<R: BloxRuntime> {
    type Handle: Clone + Send + 'static;
    fn kill(handle: Self::Handle);
}
```

Two implementations exist:
- `NoKill` (in `bloxide-core`) — for static runtimes (Embassy). `Handle = ()` (ZST), `kill` is a no-op.
- `Kill` (in `bloxide-spawn`) — for dynamic runtimes (Tokio). `Handle = R::KillHandle`, `kill` calls `R::kill(handle)` via `SpawnCap`.

The supervisor stores the cloneable `kill_handle: Option<<R::Kill as KillCapability<R>>::Handle>` per child in `ChildEntry` (populated by `add_dynamic`). When `ChildPolicy::Kill` fires, `ChildGroup::handle_done_or_failed` takes the handle out of the entry and calls `R::Kill::kill(handle)`. The handle is `R::KillHandle` (Clone), not `R::TaskHandle` (not Clone), so it can be cloned out of `&Event` in action functions and stored at registration time.

### Key Invariants for KillCapability

- KillCapability is the ripcord of last resort — for unresponsive actors or cleanup, not a replacement for the cooperative lifecycle levels (`Reset`/`Stop`/`Abort`).
- Actors never see KillCapability; only supervisors and the wiring layer hold handles.
- KillCapability is a runtime-facing capability (Tier 2), not a blox-facing trait.
- After `kill()`, no `on_exit` callbacks fire — the task is dropped in-place.
- Killed actors are permanently dead and cannot be restarted.
- `ChildPolicy::Kill` is the only policy that invokes `KillCapability::kill`. `ChildPolicy::Abort` uses the cooperative `AbortCommand` mailbox instead.
- Kill requires a dynamically spawned child: the kill handle comes from `SpawnCap::kill_handle(task_handle)` at spawn time and reaches the supervisor via `RegisterDynamicChild`.

## Generic Supervisor (`bloxide-supervisor`)

The `bloxide-supervisor` crate provides a ready-to-use supervisor as a `MachineSpec`, generated from `crates/bloxide-supervisor/blox.toml`. No custom blox is needed — the wiring layer constructs a `ChildGroup<R>`, configures per-child policies and a group-level shutdown trigger, and spawns the generic `SupervisorSpec<R>`.

### Key Types

| Type | Role | Crate |
|---|---|---|
| `SupervisorSpec<R>` | `MachineSpec` implementing the supervisor state machine | `bloxide-supervisor` (generated) |
| `SupervisorCtx<R>` | Context holding `ChildGroup<R>`, the notify ref, and the pending `ChildAction` | `bloxide-supervisor` (generated) |
| `SupervisorState` | `Running` / `ShuttingDown` | `bloxide-supervisor` (generated) |
| `SupervisorEvent<R>` | `Lifecycle` / `Child` / `Control` event enum | `bloxide-supervisor` (generated) |
| `ChildGroup<R>` | Registry of children with per-child policies and lifecycle refs | `bloxide-child-management` |
| `ChildPolicy` | Per-child lifecycle policy (what to do when a child stops or fails) | `bloxide-child-management` |
| `GroupShutdown` | Group-level trigger for entering `ShuttingDown` | `bloxide-child-management` |
| `ChildAction` | Internal signal: `Continue` or `BeginShutdown` | `bloxide-child-management` |

## Per-Child Policy (`ChildPolicy`)

Each child is registered with its own `ChildPolicy` that determines what happens when it reports `Stopped` or `Failed`:

```rust
// In bloxide-child-management
pub enum ChildPolicy {
    Reset,  // Send Reset → child goes to initial_state(), immediately operational
    Stop,   // No command sent — mark the child done for this epoch
    Abort,  // Send AbortCommand (cooperative task termination)
    Kill,   // Call KillCapability::kill (ripcord, permanently dead)
}
```

**`Reset`**: The supervisor sends `Reset` to the child. The child goes directly to `initial_state()` and reports `Started` — the supervisor records the restart via `handle_started`. No separate `Start` is needed. The group returns `ChildAction::Continue` and the child enters `ResetPending` phase until the `Started` event arrives.

**`Stop`**: **No command is sent to the child.** The child is already stopping (suspended in Init) or failed (parked — see below); the group simply marks it `Stopped` — task alive, terminal for this epoch — and evaluates the group shutdown trigger.

**`Abort`**: The supervisor sends `AbortCommand::Abort` on the child's abort mailbox. The child's task ends cooperatively. The child is marked `Aborted` immediately (the abort is fire-and-forget); the later `ChildLifecycleEvent::Aborted` is informational and recorded by `record_aborted`.

**`Kill`**: The supervisor calls `R::Kill::kill(kill_handle)` — the ripcord. The task is permanently destroyed. `ChildGroup` then **synthesizes `ChildLifecycleEvent::Killed` onto the notify channel** so the supervisor (and any observers) learn the child was killed; the supervisor records it via `record_killed`.

> `Abort` and `Kill` require a dynamically spawned child (registered via `ChildGroup::add_dynamic` with an abort mailbox and kill handle). `ChildGroup::add` **panics** if either policy is requested for a static child.

## Failed Is Not Terminal for Supervised Children

This is the foundation of the policy layer: **a supervised child's task stays alive after `Failed`.** Supervised run configs (`RunConfig::supervised`, `RunConfig::supervised_with_abort`) set `exit_on_fail = false`, so when the run loop observes `DispatchOutcome::Failed` it reports `ChildLifecycleEvent::Failed` and keeps running. The actor parks — in its absorbing error state if it has one (error states are absorbing: domain events produce `HandledNoTransition`), or in Init if the failure came via `Decision::Fail` with no `error_state()` declared — and waits.

The supervisor's `ChildPolicy` then decides the child's fate:

- `Reset` **revives the child in place** — `LifecycleCommand::Reset` sends it directly to `initial_state()` (full exit chain + entry chain, skipping Init), immediately operational, reporting `Started`. No respawn, no new channels, no wiring changes.
- `Stop` leaves it parked/suspended and counts it done for this epoch.
- `Abort` ends the parked task cooperatively.
- `Kill` destroys the parked task externally.

Root and unsupervised actors run with `exit_on_fail = true` — for them `Failed` still ends the task.

## Group Shutdown Trigger (`GroupShutdown`)

`GroupShutdown` determines when the supervisor transitions from `Running` to `ShuttingDown`:

```rust
// In bloxide-child-management
pub enum GroupShutdown {
    WhenAnyDone,   // Shut down as soon as any child is terminal
    WhenAllDone,   // Shut down only after all children are terminal
}
```

A child becomes terminal when:
- Its policy is `ChildPolicy::Stop` and it reports `Stopped` or `Failed` (no command sent; marked `Stopped` immediately — task alive), OR
- Its policy is `ChildPolicy::Abort` and it reports `Stopped` or `Failed` (the supervisor sends `AbortCommand` and marks it `Aborted` immediately), OR
- Its policy is `ChildPolicy::Kill` and it reports `Stopped` or `Failed` (the supervisor invokes the ripcord, synthesizes `Killed`, and marks it `Killed` immediately), OR
- It reports `Done` (clean self-termination via `Decision::Done`) and is deregistered — `deregister` re-evaluates the group shutdown condition so shutdown still progresses when the last registered child completes.

> **Note**: `ChildPolicy::Reset` does not make a child terminal — it sends `Reset`, which goes directly to `initial_state()` and keeps the child operational.

`check_shutdown` implements the trigger: `WhenAnyDone` returns `BeginShutdown` as soon as any single child reaches a terminal phase; `WhenAllDone` returns `BeginShutdown` only when every child is terminal (`is_terminal()` — `Stopped`/`Aborted`/`Killed`). An empty group is vacuously true, which preserves Done-deregistration shutdown.

## Child Lifecycle Triggers

The supervisor handles all seven `ChildLifecycleEvent` variants, plus the synthetic "rogue" trigger from health checks:

| Trigger | `ChildLifecycleEvent` | Meaning | Supervisor action |
|---|---|---|---|
| **Started** | `Started { child_id }` | Child exited Init or was Reset — now operational | `record_started` |
| **Stopped** | `Stopped { child_id }` | Child returned to Init via `Decision::Stop` or `LifecycleCommand::Stop` (suspended) | `handle_done_or_failed` (Running) / `record_stopped` (ShuttingDown) |
| **Done** | `Done { child_id }` | Child self-terminated cleanly via `Decision::Done` — task ended | `deregister_done` (no restart policy) |
| **Failed** | `Failed { child_id }` | Child entered an error state (`is_error()` returned `true`) or returned `Decision::Fail` — task parked, still alive | `handle_done_or_failed` |
| **Aborted** | `Aborted { child_id }` | Child's task self-terminated via `AbortCommand` | `record_aborted` |
| **Killed** | `Killed { child_id }` | Child was killed via the ripcord (synthesized by `ChildGroup`, not the child's run loop) | `record_killed` |
| **Alive** | `Alive { child_id }` | Child responded to `Ping` (healthy) | `record_alive` |
| **Rogue** | *(health tick missed)* | Child failed to respond to the previous `Ping` by the next `HealthCheckTick` | treated as `handle_done_or_failed` |

## `ChildGroup<R>` — Encapsulated Lifecycle/Shutdown Logic

`ChildGroup<R>` (in `bloxide-child-management`) encapsulates all policy evaluation and shutdown decisions. The supervisor's handler tables call methods on `ChildGroup` and inspect the returned `ChildAction` to decide state transitions.

```rust
pub struct ChildGroup<R: BloxRuntime> { /* opaque */ }

impl<R: BloxRuntime> ChildGroup<R> {
    pub fn new(shutdown: GroupShutdown) -> Self;
    /// Static child. Panics if policy is Kill or Abort (no handles available).
    pub fn add(&mut self, id: ActorId, lifecycle_ref: ActorRef<LifecycleCommand, R>, policy: ChildPolicy);
    /// Dynamically spawned child with abort mailbox + kill handle.
    pub fn add_dynamic(
        &mut self,
        id: ActorId,
        lifecycle_ref: ActorRef<LifecycleCommand, R>,
        abort_ref: ActorRef<AbortCommand, R>,
        kill_handle: <R::Kill as KillCapability<R>>::Handle,
        policy: ChildPolicy,
    );
    pub fn start_child(&self, child_id: ActorId, from: ActorId);

    pub fn start_all(&self, from: ActorId);
    /// Sends Stop to every child whose task is still alive
    /// (skips task-gone — `Aborted`/`Killed` — children; their mailboxes are dead).
    pub fn stop_all(&self, from: ActorId);

    pub fn handle_done_or_failed(
        &mut self,
        child_id: ActorId,
        from: ActorId,
        notify: &ActorRef<ChildLifecycleEvent, R>,
    ) -> ChildAction;
    pub fn handle_started(&mut self, child_id: ActorId);
    pub fn handle_alive(&mut self, child_id: ActorId);
    pub fn health_check_tick(
        &mut self,
        from: ActorId,
        notify: &ActorRef<ChildLifecycleEvent, R>,
    ) -> ChildAction;

    pub fn record_stopped(&mut self, child_id: ActorId);
    pub fn record_aborted(&mut self, child_id: ActorId);
    pub fn record_killed(&mut self, child_id: ActorId);
    /// Remove a cleanly completed child and re-evaluate group shutdown.
    pub fn deregister(&mut self, child_id: ActorId) -> ChildAction;
    pub fn all_stopped(&self) -> bool;
    /// Reset non-terminal phases for a new epoch; terminal entries keep their phase.
    pub fn clear_counters(&mut self);
}
```

`handle_done_or_failed` evaluates the child's `ChildPolicy` (four variants):
- **`ChildPolicy::Kill`** → takes the stored `kill_handle` and calls `R::Kill::kill(handle)` (the ripcord). No callbacks. Then **synthesizes `ChildLifecycleEvent::Killed { child_id }` onto `notify`** (the kill is synchronous, so the group emits the event directly — analogous to how the abort path's `Aborted` arrives later from the run loop). Marks the child `Killed`. Evaluates `GroupShutdown`.
- **`ChildPolicy::Abort`** → sends `AbortCommand::Abort { child_id }` on the child's `abort_ref` (cooperative). The child's task will self-terminate and the supervisor later receives `ChildLifecycleEvent::Aborted`. Marks the child `Aborted` immediately. Evaluates `GroupShutdown`.
- **`ChildPolicy::Reset`** → sends `Reset` to the child (goes directly to `initial_state()`, no separate `Start`), sets the child's phase to `ResetPending`. Returns `Continue`.
- **`ChildPolicy::Stop`** → sends **no command** — the child is already stopping or parked-failed. Marks the child `Stopped` (task alive, terminal for this epoch). Evaluates `GroupShutdown`.

For the `Kill`/`Abort`/`Stop` outcomes, `handle_done_or_failed` returns `BeginShutdown` when the group shutdown condition is met, otherwise `Continue`.

Children already in a terminal phase (`Stopped`/`Aborted`/`Killed`) or `ResetPending` phase are ignored (a duplicate `Stopped`/`Failed` while a Reset is in flight is coalesced).

`handle_started` records that a child has started. `Started` covers both initial `Start` (from `Init`) and `Reset` (which goes directly to `initial_state()`), so there is no separate `handle_reset` — `Reset` does not produce a distinct event. A `Started` event transitions the child out of `ResetPending` into `Running`.

`health_check_tick` implements a deterministic health-check round:
- Children that missed the previous round's `Alive` are treated as rogue (routed through `handle_done_or_failed`, so normal child policy applies)
- Currently monitored children are pinged (`LifecycleCommand::Ping`) for the next round

`record_aborted` / `record_killed` mark the child `Aborted` / `Killed` (terminal, task gone), which excludes the child from future `stop_all` sends (its mailboxes are dead) and from health monitoring. `record_stopped` sets `Stopped` but never overwrites a terminal phase — a late `Stopped` must not resurrect mailbox sends to a dead task.

## Supervisor State Machine

`SupervisorSpec<R>` has two states: `Running` (initial) and `ShuttingDown`.

```mermaid
stateDiagram-v2
    state "[engine-implicit Init]" as Init

    [*] --> Init
    Init --> Running : "dispatch(SupervisorEvent::Lifecycle(Start)) at boot"

    Running --> Running : "Stopped/Failed [policy == Reset] — child revived"
    Running --> ShuttingDown : "Stopped/Failed/Done/HealthCheckTick [GroupShutdown trigger met]"
    ShuttingDown --> [*] : "Decision::Stop (all children stopped) → Init; root run loop sees Stopped and returns"
```

When a child reports `Stopped` or `Failed`:
1. `handle_done_or_failed` evaluates the child's `ChildPolicy` and the group's `GroupShutdown`.
2. If the result is `ChildAction::Continue`, the supervisor stays in `Running` (Reset was sent, or other children still running under `WhenAllDone`).
3. If the result is `ChildAction::BeginShutdown`, the supervisor transitions to `ShuttingDown`.

Entering `ShuttingDown` runs the entry action `stop_all_children`, which sends `Stop` to every child whose task is still alive (task-gone — `Aborted`/`Killed` — children are skipped; `Stopped` children still receive `Stop` because their task is alive). In `ShuttingDown`, the supervisor counts `Stopped` events (`record_stopped`) and deregisters late `Done` completions; when `all_children_stopped()` holds, the transition guard returns `Decision::Stop` and the supervisor self-stops. That produces `DispatchOutcome::Stopped`, and the root run loop (`RunConfig::root()`, `exit_on_stop = true`) sees `Stopped` and returns — the supervisor task exits cleanly.

Note the supervisor self-stops via **`Decision::Stop`**, not Reset: there is nothing to restart to. Both `Running` and `ShuttingDown` have `Done` transitions (`deregister_done`) so clean completions are handled in either state.

**The shutdown race is silent by design.** `Stopped` children still get `Stop` intentionally (their task is alive), but a child may die *while* stopped — its run loop exits via all-streams-close and the group still holds the now-dead lifecycle channel. Sends to such already-exited tasks fail with `Closed`, which `stop_all` / `start_all` / `start_child` treat as a silent no-op via `BloxRuntime::try_send_error_is_closed`; only genuine backpressure (`Full`) logs a warning. The reporting direction classifies the same way: a child's `report_outcome` to an already-exited supervisor is dropped silently, while a full notify channel still warns.

### `SupervisorCtx<R>`

Generated from `blox.toml` — four fields, three constructor args (`pending` is a state field):

```rust
pub struct SupervisorCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub children: ChildGroup<R>,
    pub child_notify: ActorRef<ChildLifecycleEvent, R>,
    pub pending: ChildAction,
}

impl<R: BloxRuntime> SupervisorCtx<R> {
    pub fn new(
        self_id: ActorId,
        children: ChildGroup<R>,
        child_notify: ActorRef<ChildLifecycleEvent, R>,
    ) -> Self;
}
```

`child_notify` is the group's own notify channel ref — the action functions pass it to `ChildGroup::handle_done_or_failed` / `health_check_tick` so a synthesized `Killed` event (or a rogue-child policy outcome) lands back on the supervisor's notify mailbox.

### Handler Tables

> The transition rules below are declared as `[[topology.transitions]]` entries in `blox.toml` and emitted as raw `StateRule { ... }` struct literals by `bloxide-codegen`. The rule structure (event match, actions, guard, targets) is shown in TOML form, mirroring `crates/bloxide-supervisor/blox.toml`.

```toml
# RUNNING state
[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))"
target = "stay"
actions = ["Self::handle_done_or_failed"]
guards = [{ condition = "ctx.pending == ChildAction::BeginShutdown", target = "ShuttingDown" }]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Failed { .. }))"
target = "stay"
actions = ["Self::handle_done_or_failed"]
guards = [{ condition = "ctx.pending == ChildAction::BeginShutdown", target = "ShuttingDown" }]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Started { .. }))"
target = "stay"
actions = ["Self::record_started"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Aborted { .. }))"
target = "stay"
actions = ["Self::record_aborted"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Killed { .. }))"
target = "stay"
actions = ["Self::record_killed"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Alive { .. }))"
target = "stay"
actions = ["Self::record_alive"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Done { .. }))"
target = "stay"
actions = ["Self::deregister_done"]
guards = [{ condition = "ctx.pending == ChildAction::BeginShutdown", target = "ShuttingDown" }]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Control(Envelope(_, ChildCtrl::RegisterChild(_)))"
target = "stay"
actions = ["Self::register_child"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Control(Envelope(_, ChildCtrl::RegisterDynamicChild(_)))"
target = "stay"
actions = ["Self::handle_register_dynamic_child"]

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Control(Envelope(_, ChildCtrl::HealthCheckTick))"
target = "stay"
actions = ["Self::handle_health_check"]
guards = [{ condition = "ctx.pending == ChildAction::BeginShutdown", target = "ShuttingDown" }]

# Catch-alls: absorb any other Child / Control events
[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Child(_)"
target = "stay"

[[topology.transitions]]
state = "Running"
event = "SupervisorEvent::Control(_)"
target = "stay"

# SHUTTING_DOWN state
[[topology.transitions]]
state = "ShuttingDown"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))"
target = "stay"
actions = ["Self::record_stopped"]
guards = [{ condition = "ctx.all_children_stopped()", target = "stop" }]

[[topology.transitions]]
state = "ShuttingDown"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Done { .. }))"
target = "stay"
actions = ["Self::deregister_done"]
guards = [{ condition = "ctx.all_children_stopped()", target = "stop" }]

[[topology.transitions]]
state = "ShuttingDown"
event = "SupervisorEvent::Child(_)"
target = "stay"

[[topology.transitions]]
state = "ShuttingDown"
event = "SupervisorEvent::Control(_)"
target = "stay"

# Entry actions
[[topology.entry]]
state = "Running"
actions = ["Self::start_children"]

[[topology.entry]]
state = "ShuttingDown"
actions = ["Self::stop_all_children"]
```

A transition guard with `target = "stop"` is the declarative form of `Decision::Stop` — the supervisor self-stops when all children have stopped.

### Action Functions

All supervisor actions are free functions in `bloxide-child-management::actions`, returning `ActionResult` and taking concrete params (extracted context fields) plus an extracted event payload — never the consumer's event enum (spec 20: Platform Feature Pattern). The generated wrapper closures extract the fields from `SupervisorCtx` and call them:

| Function | Signature (params in order) | Purpose |
|---|---|---|
| `start_children` | `(self_id, &mut ChildGroup, &mut ChildAction)` | Running on_entry: clear counters, reset pending, `start_all` |
| `stop_all_children` | `(self_id, &ChildGroup)` | ShuttingDown on_entry: `stop_all` (skips task-gone — `Aborted`/`Killed` — children) |
| `handle_done_or_failed` | `(self_id, &mut ChildGroup, &ActorRef<ChildLifecycleEvent, R>, &mut ChildAction, &ChildLifecycleEvent)` | Apply child policy on `Stopped`/`Failed`; store resulting `ChildAction` in `pending` |
| `record_started` | `(&mut ChildGroup, &ChildLifecycleEvent)` | Child operational |
| `record_stopped` | `(&mut ChildGroup, &ChildLifecycleEvent)` | Count stops in ShuttingDown |
| `record_aborted` | `(&mut ChildGroup, &ChildLifecycleEvent)` | Mark task gone (cooperative end) |
| `record_killed` | `(&mut ChildGroup, &ChildLifecycleEvent)` | Mark task gone (ripcord) |
| `record_alive` | `(&mut ChildGroup, &ChildLifecycleEvent)` | Clear pending health bit |
| `deregister_done` | `(&mut ChildGroup, &mut ChildAction, &ChildLifecycleEvent)` | Remove cleanly completed child; record shutdown decision in `pending` |
| `register_child` | `(self_id, &mut ChildGroup, &ChildCtrl)` | `ChildGroup::add` + `start_child` |
| `handle_register_dynamic_child` | `(self_id, &mut ChildGroup, &ChildCtrl)` | `ChildGroup::add_dynamic` (stores `abort_ref` + `kill_handle`) + `start_child` |
| `handle_health_check` | `(self_id, &mut ChildGroup, &ActorRef<ChildLifecycleEvent, R>, &mut ChildAction, &ChildCtrl)` | Run one health-check round; store resulting `ChildAction` in `pending` |

### `MachineSpec` Implementation

```rust
impl<R: BloxRuntime> MachineSpec for SupervisorSpec<R> {
    type State = SupervisorState;
    type Event = SupervisorEvent<R>;
    type Ctx = SupervisorCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (
        Rt::Stream<ChildLifecycleEvent>,
        Rt::Stream<ChildCtrl<R>>,
    );

    fn initial_state() -> SupervisorState { SupervisorState::Running }

    // on_init_entry fires only when the supervisor itself is Stopped (enters
    // Init). In the four-level model, Decision::Reset goes directly to
    // initial_state() (Running) — it does NOT fire on_init_entry. Counter
    // clearing for a normal restart cycle is therefore done by the Running
    // on_entry action `start_children`, not here.
    fn on_init_entry(ctx: &mut SupervisorCtx<R>) {
        ctx.children.clear_counters();
        ctx.pending = ChildAction::default();
    }
}
```

The **Running on_entry** action is `start_children` (in `bloxide-child-management::actions`). It calls `ctx.children.clear_counters()`, resets `ctx.pending` to `ChildAction::default()`, and then calls `start_all` to send `Start` to every child. `clear_counters` resets non-terminal entries to `Init` for the new epoch but skips terminal entries — as a design note, resetting a killed/aborted child to `Init` would make it health-monitored again, sending a `Ping` to a dead mailbox and firing a spurious second `Kill` when the `Ping` goes unanswered. Because `Decision::Reset` goes directly to `initial_state()` (Running), this on_entry fires both on the initial `Start` from wiring and on any `Decision::Reset` — replacing the old `on_init_entry` counter-clearing for the restart cycle.

## Lifecycle Flow

### Reset path (ChildPolicy::Reset)

```mermaid
sequenceDiagram
    participant Sup as SupervisorSpec
    participant CG as ChildGroup
    participant RT as Child Run Loop
    participant M as Child StateMachine

    Note over M: Actor runs, processing domain events...

    M-->>RT: DispatchOutcome::Failed (error state) or ::Stopped (Decision::Stop)
    RT-->>Sup: ChildLifecycleEvent::Failed / Stopped
    Sup->>CG: handle_done_or_failed(child_id)
    Note over CG: policy == Reset
    CG->>RT: LifecycleCommand::Reset
    CG-->>Sup: ChildAction::Continue
    RT->>M: handle_lifecycle(Reset)
    Note over RT: Reset goes directly to initial_state() (skips Init)
    Note over RT: Returns DispatchOutcome::Started(initial_state)
    RT-->>Sup: ChildLifecycleEvent::Started
    Sup->>CG: handle_started(child_id)
    Note over CG: Record child as running (ResetPending → Running).
```

### Kill path (ChildPolicy::Kill)

```mermaid
sequenceDiagram
    participant Sup as SupervisorSpec
    participant CG as ChildGroup
    participant RT as Child Run Loop

    RT-->>Sup: ChildLifecycleEvent::Failed (or Stopped, or missed Alive)
    Sup->>CG: handle_done_or_failed(child_id)
    Note over CG: policy == Kill
    CG->>CG: R::Kill::kill(kill_handle) — ripcord, task destroyed in place
    Note over RT: Task gone — never reports an outcome
    CG-->>Sup: synthesizes ChildLifecycleEvent::Killed onto the notify channel
    CG-->>Sup: ChildAction::BeginShutdown (if GroupShutdown met)
    Sup->>CG: record_killed(child_id)
    Note over CG: terminal phase (Stopped/Aborted/Killed)
```

### Shutdown path (GroupShutdown trigger met)

```mermaid
sequenceDiagram
    participant Sup as SupervisorSpec
    participant CG as ChildGroup
    participant RT as Child Run Loop
    participant M as Child StateMachine

    Note over M: Actor runs, processing domain events...

    M-->>RT: DispatchOutcome::Failed / Stopped / Done
    RT-->>Sup: ChildLifecycleEvent::Failed / Stopped / Done
    Sup->>CG: handle_done_or_failed(child_id) / deregister(child_id)
    Note over CG: child terminal, GroupShutdown condition met
    CG-->>Sup: ChildAction::BeginShutdown
    Note over Sup: Running → ShuttingDown; on_entry: stop_all_children
    Sup->>CG: stop_all(from)
    CG->>RT: LifecycleCommand::Stop (to each live child; task-gone (Aborted/Killed) skipped)
    RT->>M: handle_lifecycle(Stop)
    Note over RT: Child suspends to Init; task stays alive
    RT-->>Sup: ChildLifecycleEvent::Stopped
    Sup->>CG: record_stopped(child_id)
    Note over Sup: all_children_stopped() → Decision::Stop
    Note over Sup: Supervisor self-stops; root run loop sees Stopped and exits
```

## Health Checks (implemented)

Health checks are delivered through the supervisor control-plane stream:

1. A health driver (for example, a runtime timer task) sends `ChildCtrl::HealthCheckTick`.
2. The supervisor calls `health_check_tick(from, notify)` on `ChildGroup` via the `handle_health_check` action.
3. `ChildGroup` marks children that missed the previous `Alive` as rogue and applies normal child policy (`handle_done_or_failed`).
4. `ChildGroup` sends `LifecycleCommand::Ping` to currently monitored children.
5. Children reply with `ChildLifecycleEvent::Alive { child_id }`, clearing the pending health bit.

This is intentionally externalized: `bloxide-child-management` defines the protocol, while wiring/runtime code chooses how ticks are produced.

**Known limitation**: In Embassy's cooperative scheduler, a truly stuck actor (infinite loop, blocking call) will never yield to process the `Ping` command. Health checks can only detect actors whose run loop has stalled while awaiting — not actors that never await.

## `ChildLifecycleEvent`

Defined in `bloxide-core::lifecycle`. The runtime generates these automatically by observing `DispatchOutcome` (via `report_outcome` in `bloxide-core::supervision`) — no actor code sends them. The single exception is `Killed`, which `ChildGroup` synthesizes directly onto the notify channel because a killed task never runs again to report anything.

```rust
pub enum ChildLifecycleEvent {
    Started { child_id: ActorId },  // child exited Init or was Reset (now operational)
    Failed  { child_id: ActorId },  // child entered an error state (is_error) or returned Decision::Fail
    Stopped { child_id: ActorId },  // child was Stopped (Decision::Stop or LifecycleCommand::Stop), now in Init (suspended)
    Done    { child_id: ActorId },  // child self-terminated cleanly via Decision::Done (task ended — deregister, no restart)
    Aborted { child_id: ActorId },  // child was Aborted, task has ended (cooperative)
    Killed  { child_id: ActorId },  // child was killed via KillCapability (external destruction; synthesized by ChildGroup)
    Alive   { child_id: ActorId },  // child responded to Ping (healthy)
}
```

> **Note**: `Done` is clean self-termination — not a return to terminal states
> (there is still no `is_terminal`). `Decision::Done` runs the same Init
> cleanup ritual as `Decision::Stop` (exit chain + `on_init_entry`), then the
> run loop ends the task (`DispatchOutcome::Done` always exits). The
> supervisor deregisters the child via `deregister_done`; no `ChildPolicy`
> restart fires. Use `Decision::Stop` for suspend/resume, `Decision::Done`
> for normal completion.

## `LifecycleCommand`

Defined in `bloxide-core::lifecycle`. Sent by the supervisor (via `ChildGroup`) to each child's runtime-internal lifecycle channel.

```rust
pub enum LifecycleCommand {
    Start,
    Reset,
    Stop,
    Ping,
}
```

| Command | Runtime behavior |
|---|---|
| `Start` | Init → `initial_state()` (already operational → no-op) |
| `Reset` | Full exit chain → `initial_state()` directly (skips Init, reports `Started`; from Init this is equivalent to `Start`) |
| `Stop` | Full exit chain → Init + `on_init_entry`, task stays alive suspended (already in Init → acknowledges with `Stopped` so shutdown counting works) |
| `Ping` | Child responds with `ChildLifecycleEvent::Alive` |

## Supervised Actor Run Loop

All runtimes share the unified actor run loop — `run()` in `bloxide-core`
(`crates/bloxide-core/src/runloop.rs`), re-exported by each runtime. A
supervised child runs with `RunConfig::supervised(lifecycle_rx, notify)` or, when
the runtime supports kill, `RunConfig::supervised_with_abort(lifecycle_rx,
abort_rx, notify)`, which adds an abort mailbox for cooperative self-termination.
This is wiring-layer code — never used as a bound on blox crates.

`RunConfig` fields control the loop:

| Field | Supervised child | Root / unsupervised |
|---|---|---|
| `lifecycle` | `Some` (commands from supervisor) | `None` |
| `abort` | `Some` only with `supervised_with_abort` | `None` |
| `supervisor_notify` | `Some` | `None` |
| `auto_start` | `false` (waits for `Start`) | `false` for root (wired dispatch), `true` for unsupervised |
| `exit_on_stop` | **`false` — stays alive suspended in Init** | `true` |
| `exit_on_fail` | **`false` — parks in its error state, supervisor's policy decides** | `true` |

The run loop polls streams in priority order:
1. **Lifecycle stream** (`LifecycleCommand`: `Start`/`Reset`/`Stop`/`Ping`) — highest priority.
2. **Abort mailbox** (`AbortCommand::Abort`, only with `supervised_with_abort`) — serviced before domain messages so a stuck actor can be terminated promptly when it next yields. On receipt, the run loop reports `DispatchOutcome::Aborted` to the supervisor and self-terminates (no `dispatch()`, no callbacks).
3. **Domain mailboxes** — polled only when no lifecycle or abort command is pending.

After every dispatch, the run loop calls `report_outcome`, which translates the `DispatchOutcome` into the corresponding `ChildLifecycleEvent` and sends it to the supervisor automatically (a full/closed supervisor channel logs a warning and drops the event — supervision never blocks the run loop). The `Aborted` outcome is synthesized by the run loop itself (not by `dispatch()`), since `Abort` bypasses the dispatch pipeline.

`DispatchOutcome` has **no `Killed` variant** — a killed task never runs again, so there is nothing to observe. The loop always exits on `Aborted` and `Done`, regardless of config.

## `SupervisorEvent` and `ChildCtrl`

The generated event enum for the supervisor (`crates/bloxide-supervisor/src/generated/events.rs`) has three variants; `Child` and `Control` carry `Envelope` wrappers (sender id + payload), while `Lifecycle` carries the raw command:

```rust
pub enum SupervisorEvent<R: BloxRuntime> {
    /// Lifecycle command (Start/Reset/Stop/Ping) — handled at VirtualRoot.
    Lifecycle(LifecycleCommand),
    Child(Envelope<ChildLifecycleEvent>),
    Control(Envelope<ChildCtrl<R>>),
}
```

The `Lifecycle` variant is how the wiring layer boots the supervisor (`dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start))`) — like every actor, the supervisor receives its own lifecycle commands through dispatch.

`ChildCtrl` (in `bloxide-child-management::control`) is the control-plane protocol:

```rust
pub enum ChildCtrl<R: BloxRuntime> {
    RegisterChild(RegisterChild<R>),                  // static child: id + lifecycle_ref + policy
    RegisterDynamicChild(RegisterDynamicChild<R>),    // dynamic child: + abort_ref + kill_handle
    HealthCheckTick,
}
```

`Child` variants arrive from the runtime's supervised run loop. `Control` variants come from supervisor wiring/control-plane senders and enable:
- static registration of supervised children (`RegisterChild`)
- dynamic registration of supervised children with abort/kill capability (`RegisterDynamicChild` — carries the `abort_ref` and `kill_handle` needed by `ChildPolicy::Abort` and `ChildPolicy::Kill`; sent by the `spawn_child` helper after a dynamic spawn)
- periodic health checks (`HealthCheckTick`)

## Dynamic Spawning and Registration

Dynamic children are created by the **requesting** blox (e.g. a pool), not by the supervisor. The `spawn_child` helper in `bloxide-spawn` ties the pieces together:

```rust
// In bloxide-spawn
pub type SpawnFn<R, Req> = fn(req: Req, notify: ActorRef<ChildLifecycleEvent, R>) -> SpawnOutput<R>;

pub struct SpawnOutput<R: BloxRuntime> {
    pub child_id: ActorId,
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    pub abort_ref: ActorRef<AbortCommand, R>,
    pub kill_handle: <R::Kill as KillCapability<R>>::Handle,
    pub policy: ChildPolicy,
}

pub fn spawn_child<R, Req, C>(
    spawn_fn: SpawnFn<R, Req>,
    req: Req,
    control_ref: &ActorRef<C::RegisterMsg, R>,
    notify_ref: &ActorRef<ChildLifecycleEvent, R>,
    from: ActorId,
) -> Result<(), R::TrySendError>;
```

`spawn_child` calls the application-provided spawn function (which allocates channels, builds the child, spawns the task with `RunConfig::supervised_with_abort`, and converts the `TaskHandle` into a cloneable `KillHandle` via `SpawnCap::kill_handle`), then wraps the returned `SpawnOutput` into the managing blox's registration message via a `ChildRegistrar` and sends it on the control mailbox. `ChildCtrlRegistrar` is the registrar for the standard control plane — it wraps `SpawnOutput` into `ChildCtrl::RegisterDynamicChild`. The supervisor's `handle_register_dynamic_child` action then calls `ChildGroup::add_dynamic` and starts the child.

See `13-factory-injection-and-supervision.md` for the full factory-injection walkthrough.

## Wiring a Supervised Group

The wiring layer uses `ChildGroupBuilder` and the runtime's `spawn_child!` macro — no custom blox is needed. This example mirrors the generated `apps/tokio-demo/src/main.rs`:

```rust
use bloxide_tokio::prelude::*;  // ChildGroupBuilder, GroupShutdown, ChildPolicy, ...

// Domain channels for children
let ((ping_ref,), ping_mbox) = ::bloxide_tokio::channels! { PingPongMsg(16) };
let ping_id = ping_ref.id();
let ((pong_ref,), pong_mbox) = ::bloxide_tokio::channels! { PingPongMsg(16) };
let pong_id = pong_ref.id();

// ChildGroupBuilder allocates the notify + control channels.
// Grab the control/notify refs before finish() consumes the builder.
let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
let sup_control_ref = group.control_ref();
let sup_notify_ref = group.notify_ref();

// Build contexts and wrap them in state machines (timer_ref omitted for brevity)
let ping_ctx = PingCtx::new(ping_id, pong_ref.clone(), ping_ref.clone(), timer_ref.clone());
let pong_ctx = PongCtx::new(pong_id, ping_ref.clone());
let ping_machine = ::bloxide_core::StateMachine::new(ping_ctx);
let pong_machine = ::bloxide_core::StateMachine::new(pong_ctx);

// spawn_child! registers each child with the group and spawns its task
::bloxide_tokio::spawn_child!(
    group,
    ping_task(ping_machine, ping_mbox, ping_id),
    ChildPolicy::Stop
);
::bloxide_tokio::spawn_child!(
    group,
    pong_task(pong_machine, pong_mbox, pong_id),
    ChildPolicy::Stop
);

let sup_id = ::bloxide_tokio::next_actor_id!();
let (children, sup_notify_rx, sup_control_rx) = group.finish();

// The generic supervisor — 3-arg context constructor
let sup_ctx = ::bloxide_supervisor::SupervisorCtx::new(sup_id, children, sup_notify_ref);
let mut sup_machine = ::bloxide_core::StateMachine::<
    crate::generated::bloxide_supervisor_spec_skeleton::SupervisorSpec<TokioRuntime>,
>::new(sup_ctx);

// Boot: dispatch Start through the Lifecycle variant of the generated event enum
sup_machine.dispatch(
    SupervisorEvent::<TokioRuntime>::Lifecycle(LifecycleCommand::Start),
);

// The supervisor runs as the root task (RunConfig::root) — when it
// self-stops (Decision::Stop), the run loop returns and main() completes
supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;
```

Important details:

- The spec type is the **system-generated concrete spec** (`crate::generated::bloxide_supervisor_spec_skeleton::SupervisorSpec`) — the system-level codegen emits it with real action closures wired to `bloxide-child-management::actions`. Apps never use the blox-crate-level stub spec.
- `SupervisorCtx::new` takes three args: `(sup_id, children, sup_notify_ref)`.
- Embassy wiring is identical in shape (`apps/embassy-demo/src/main.rs`): the same `ChildGroupBuilder::new(...)` call resolves to the shared `bloxide_child_management::ChildGroupBuilder` (re-exported by `bloxide-embassy`), which reaches Embassy channels via `GroupChannelCap`; `spawn_child!` additionally takes the Embassy `spawner`.

### One `ChildGroupBuilder` Across Runtimes

A single `ChildGroupBuilder<R: GroupChannelCap, Ctrl>` in `bloxide-child-management::builder` serves every runtime, with one API shape (`new` / `add_child` / `control_ref` / `notify_ref` / `notify_sender` / `finish`). The runtime supplies the group channels through the `GroupChannelCap` capability trait (`alloc_group_id()` + `group_channel::<M, N>(id)`): Tokio and TestRuntime forward to `DynamicChannelCap`, while Embassy forwards to `StaticChannelCap`. On Embassy, `alloc_group_id()` expands the compile-time counter once, so the notify and control channels share one baked ID — they are both mailboxes of the one logical group actor.

Generated wiring (`ChildGroupBuilder::new(...)`) is identical across runtimes because it is literally the same type — `bloxide-tokio` and `bloxide-embassy` both re-export it at the crate root and in their preludes. The builder is generic over the control message type `Ctrl` — the runtime never names `ChildCtrl`; the app chooses it.

### Supervision in `system.toml`

System-level wiring declares the supervisor and its children declaratively:

```toml
[[supervision]]
supervisor = "bloxide-supervisor"
strategy = "when_any_done"        # or "when_all_done"
children = ["ping", "pong"]

  [supervision.policies]
  ping = { stop = true }           # ChildPolicy::Stop
  pong = { restart = { max = 3 } } # ChildPolicy::Reset
```

The only valid strategies are `when_any_done` and `when_all_done` (mapping to `GroupShutdown::WhenAnyDone` / `GroupShutdown::WhenAllDone`) — any other value is a hard codegen error. In `[supervision.policies]`, `restart = { max = N }` maps to `ChildPolicy::Reset`, `stop = true` maps to `ChildPolicy::Stop`, and a child with no entry defaults to `ChildPolicy::Stop`.

## Supervision Tree

Supervisors can themselves be children of another supervisor:

```
Root Supervisor
├── Ping Actor (child, Reset)
├── Pong Actor (child, Stop)
└── Sub-Supervisor (child, Reset)
    └── ...
```

The root supervisor is bootstrapped with `sup_machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start))` in the wiring binary.

## Key Invariants

> **See `AGENTS.md` → "Key Invariants" for the canonical list.**

Supervision-specific invariants:

- Actors never see `LifecycleCommand` — it is runtime-internal.
- Actors have no `supervisor_ref` — they don't know their supervisor exists.
- `on_init_entry` is for domain-state reset only and fires only on `Stop` (entering Init). It does NOT fire on `Reset` (which skips Init and goes directly to `initial_state()`). It also fires when `Decision::Stop` or `Decision::Done` triggers (cleanup before the machine returns to Init / the task ends).
- **Four-level lifecycle**: `Reset` goes directly to `initial_state()` (task stays alive, immediately operational, reports `Started`); `Stop` goes to `Init` (task suspended, reports `Stopped`); `Abort` ends the task cooperatively via the abort mailbox (reports `Aborted`); `Kill` destroys the task in place via `KillCapability::kill` — and `ChildGroup` synthesizes `ChildLifecycleEvent::Killed` onto the notify channel so the supervisor records it via `record_killed`. Self-initiated clean exit: `Decision::Done` (exit chain + `on_init_entry`, then the task ends, reports `Done`; supervisor deregisters, no restart).
- **Supervised children survive `Stopped` and `Failed`**: their run configs set `exit_on_stop = false` / `exit_on_fail = false` — the task stays alive (suspended in Init, or parked in its absorbing error state) so the supervisor's `ChildPolicy` can revive it (`Reset`), leave it (`Stop`), or end it (`Abort`/`Kill`). Root/unsupervised actors exit on both.
- **`Decision::Stop` replaces terminal states**: When a decision returns `Stop`, the transition's actions run first, then the machine goes to `Init` and produces `DispatchOutcome::Stopped`. `on_init_entry` fires to clear state. Supervised actor run loops do NOT exit on `Stopped` — the actor stays alive in `Init`, waiting for `Start` or `Reset` from the supervisor.
- **`Decision::Done` is clean self-termination**: same cleanup ritual as `Stop` (exit chain + `on_init_entry`), but produces `DispatchOutcome::Done` and the run loop ALWAYS exits — the task ends. The supervisor deregisters the child via `deregister_done` (no `ChildPolicy` restart). Use `Stop` for suspend/resume, `Done` for normal completion.
- `Decision::Reset` goes directly to `initial_state()`, skipping Init entirely. It fires the full exit chain for the current state, then the entry chain for `initial_state()`. It does NOT call `on_init_entry` or `on_init_exit`.
- The supervisor self-stops via `Decision::Stop` when all children have stopped — never via Reset.
- Each child runs in its own task — precise per-actor wakeup is preserved.
- `ChildGroup<R>` encapsulates all policy evaluation and shutdown logic.
- Per-child `ChildPolicy` (four variants: `Reset`, `Stop`, `Abort`, `Kill`) gives each child its own lifecycle policy. `Abort`/`Kill` panic at registration for static children (`ChildGroup::add` asserts) — they require dynamically spawned children with abort/kill handles.
- `GroupShutdown` controls when the supervisor enters shutdown, not which children are affected.
- `ChildPhase` tracks each child's state: `Init`, `Running`, `ResetPending` (Reset sent, awaiting `Started`), `Stopped` (task alive — self-stopped suspended in Init, failed parked in its error state, or marked done by the `Stop` policy; terminal for the epoch), `Aborted` and `Killed` (task gone — mailboxes are dead, so lifecycle commands must not be sent). `is_terminal()` (`Stopped`/`Aborted`/`Killed`) drives group-shutdown evaluation; `is_task_gone()` (`Aborted`/`Killed`) excludes children from `stop_all`. Health checks (`is_health_monitored`) monitor only `Init`/`Running` children — terminal phases are done for the epoch, and `ResetPending` children are transitioning.
- `LifecycleCommand`, `ChildLifecycleEvent`, and `AbortCommand` are defined in `bloxide-core::lifecycle`. `ChildPolicy`, `ChildAction`, `GroupShutdown`, `ChildGroup`, and the supervision action functions are defined in `bloxide-child-management`. `ChildCtrl`, `RegisterChild`, and `RegisterDynamicChild` are defined in `bloxide-child-management::control`; `SpawnCap`, `SpawnFn`, `SpawnOutput`, `ChildCtrlRegistrar`, and `spawn_child` are defined in `bloxide-spawn` (spec 20: Platform Feature Pattern).
- No custom supervisor implementation is needed — `SupervisorSpec<R>` is a generic, reusable `MachineSpec`.

## Related Docs

- **Lifecycle engine details** → `spec/architecture/02-hsm-engine.md`
- **Wiring supervised actors** → `spec/architecture/04-static-wiring.md`
- **Supervisor as reusable blox** → `spec/architecture/12-action-crate-pattern.md` → "Supervisor As The Same Pattern"
- **Factory injection + dynamic spawning** → `spec/architecture/13-factory-injection-and-supervision.md`
- **Runtime supervision impl** → `runtimes/*/src/supervision.rs`
