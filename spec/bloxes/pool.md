# Blox Spec: `Pool`

## Purpose

The Pool actor demonstrates dynamic actor spawning with an asynchronous, two-phase spawn flow:

- Receives `PoolMsg::SpawnWorker` commands, sends a `SpawnRequest` to the injected spawn factory, and waits in `Spawning` for the `SpawnedWorker` reply on a dedicated reply mailbox
- Buffers additional `SpawnWorker` requests in a `spawn_queue` while a spawn is in flight, and processes the queue as replies arrive
- Introduces each new worker to the existing ones via `bloxide_peers::introduce_peers` (bidirectional `PeerCtrl::AddPeer`), then dispatches `WorkerMsg::DoWork`
- Tracks outstanding work in `pending`; self-suspends via `Decision::Stop` when all workers report done

This blox showcases:

- **Factory injection**: the worker spawn function (`SpawnFn`) is injected as a constructor parameter at wiring time
- **Async two-phase spawn**: request sent in one state, reply handled in another — the state machine never blocks
- **Feature gating**: the entire spawn machinery is gated behind the pool crate's `dynamic` feature
- **Dynamic actor creation**: the runtime's `SpawnCap` capability at wiring time (Tokio)

## Crate Location

- Blox crate: `crates/bloxes/pool/`
- Messages crate: `crates/messages/pool-messages/` (plain data only — `PoolMsg`, `WorkerMsg`; no `ActorRef`)
- Context crate: `crates/context/blox-ctx-pool-ref/` (provides `SpawnRequest` / `SpawnedWorker` — they carry `ActorRef`s, so they live in this domain context crate rather than in `pool-messages`)
- Impl crate: `crates/impl/tokio-pool-demo-impl/` (provides all pool action functions and the worker spawn factory)

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Idle : dispatch(Start)
    Idle --> Spawning : PoolMsg::SpawnWorker (dynamic)
    Spawning --> Active : SpawnReply [no spawn in flight, queue empty]
    Active --> Spawning : PoolMsg::SpawnWorker (dynamic)
    Active --> [*] : PoolMsg::WorkDone [pending == 0] : Decision::Stop
    note right of Spawning : SpawnReply guards — spawn_in_flight or queue non-empty → stay Spawning; pending == 0 with workers present → Decision::Stop. SpawnWorker while Spawning → stay (buffered in spawn_queue); WorkDone while Spawning → stay.
```

> `[Init]` is engine-implicit. `Idle`, `Spawning`, and `Active` are flat leaf states (no parent).

## blox.toml

The full declarative source is `crates/bloxes/pool/blox.toml`. Key excerpts:

```toml
# Two mailboxes: Msg (always) and SpawnReply (dynamic-only — carries
# SpawnedWorker replies from the spawn factory).
[event]
name = "PoolEvent"
generics = "<R: BloxRuntime>"
feature = "dynamic"
feature_generics = "<R: BloxRuntime>"

[[event.mailboxes]]
variant = "Msg"
message = "PoolMsg"
message_path = "pool_messages::PoolMsg"

[[event.mailboxes]]
variant = "SpawnReply"
message = "SpawnedWorker"
message_path = "blox_ctx_pool_ref::SpawnedWorker<bloxide_peers::PeerCtrl<pool_messages::WorkerMsg, R>, R>"
feature = "dynamic"

# Idle: SpawnWorker → Spawning (dynamic only)
[[topology.transitions]]
state = "Idle"
event = "PoolMsg::SpawnWorker(_)"
target = "Spawning"
actions = ["Self::handle_spawn_worker"]
feature = "dynamic"

# Spawning: SpawnedWorker reply → Active (or stay Spawning if more spawns
# in-flight/queued, or stop if all workers already finished)
[[topology.transitions]]
state = "Spawning"
event = "PoolEvent::SpawnReply(_)"
target = "Active"
actions = ["Self::handle_spawned_worker"]
feature = "dynamic"

[[topology.transitions.guards]]
condition = "ctx.spawn_in_flight || !ctx.spawn_queue.is_empty()"
target = "Spawning"

[[topology.transitions.guards]]
condition = "ctx.pending == 0 && !ctx.worker_refs.is_empty()"
target = "stop"

# Spawning: buffer additional SpawnWorker requests while waiting for reply
[[topology.transitions]]
state = "Spawning"
event = "PoolMsg::SpawnWorker(_)"
target = "stay"
actions = ["Self::handle_spawn_worker_queued"]
feature = "dynamic"

# Active: WorkDone with guard
[[topology.transitions]]
state = "Active"
event = "PoolMsg::WorkDone(_)"
target = "stay"
actions = ["Self::handle_work_done"]

[[topology.transitions.guards]]
condition = "ctx.pending == 0"
target = "stop"
```

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for `dispatch(Start)`; `on_init_entry` clears `worker_refs` and `worker_ctrls` and sets `pending = 0` |
| `Idle` | leaf, initial | No spawn requested yet |
| `Spawning` | leaf | Spawn request sent to the factory; awaiting the `SpawnedWorker` reply; further `SpawnWorker` requests are buffered in `spawn_queue` |
| `Active` | leaf | At least one worker running; accepts more spawns and `WorkDone` |

## Events

| Event | Handled by | Rule pattern | Guard outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| `PoolMsg::SpawnWorker(_)` | `Idle` | Action-Then-Guard | `Decision::Transition(Spawning)` (fixed) | `handle_spawn_worker` (dynamic only): `pending += 1`, `spawn_in_flight = true`, sends `SpawnRequest` to the factory |
| `PoolEvent::SpawnReply(_)` | `Spawning` | Action-Then-Guard | stay `Spawning` if `spawn_in_flight` or queue non-empty; `Decision::Stop` if `pending == 0` with workers present; else `Decision::Transition(Active)` | `handle_spawned_worker` (dynamic only): introduces peers, sends `DoWork`, stores refs, pops the next queued spawn |
| `PoolMsg::SpawnWorker(_)` | `Spawning` | Action-Then-Stay | `Decision::Stay` | `handle_spawn_worker_queued` (dynamic only): pushes `task_id` onto `spawn_queue`, `pending += 1` |
| `PoolMsg::WorkDone(_)` | `Spawning` | Action-Then-Stay | `Decision::Stay` | `handle_work_done`: `pending -= 1` |
| `PoolMsg::SpawnWorker(_)` | `Active` | Action-Then-Guard | `Decision::Transition(Spawning)` (fixed) | `handle_spawn_worker` (dynamic only), as in `Idle` |
| `PoolMsg::WorkDone(_)` | `Active` | Action-Then-Guard | `Decision::Stop` if `pending == 0`, else `Decision::Stay` | `handle_work_done`: `pending -= 1` |
| any unhandled | root (no rules) | — | dropped | none |

## Context

`PoolCtx` has paired `#[cfg]` generation: the non-dynamic variant excludes the spawn fields, spawn imports, and spawn transitions.

```rust
// Without feature "dynamic":
pub struct PoolCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub self_ref: ActorRef<PoolMsg, R>,
    pub worker_refs: Vec<ActorRef<WorkerMsg, R>>,
    pub worker_ctrls: Vec<ActorRef<PeerCtrl<WorkerMsg, R>, R>>,
    pub pending: u32,
}
// new(self_id, self_ref)

// With feature "dynamic":
pub struct PoolCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub self_ref: ActorRef<PoolMsg, R>,
    pub spawn_fn: SpawnFn<R, SpawnRequest<PeerCtrl<WorkerMsg, R>, R>>,
    pub spawn_ref: ActorRef<ChildCtrl<R>, R>,
    pub notify_ref: ActorRef<ChildLifecycleEvent, R>,
    pub spawn_reply_ref: ActorRef<SpawnedWorker<PeerCtrl<WorkerMsg, R>, R>, R>,
    pub pending_task_id: u32,
    pub spawn_in_flight: bool,
    pub spawn_queue: Vec<u32>,
    pub worker_refs: Vec<ActorRef<WorkerMsg, R>>,
    pub worker_ctrls: Vec<ActorRef<PeerCtrl<WorkerMsg, R>, R>>,
    pub pending: u32,
}
// new(self_id, self_ref, spawn_fn, spawn_ref, notify_ref, spawn_reply_ref)
```

| Field | Type | Role | Description |
|-------|------|------|-------------|
| `self_id` | `ActorId` | auto-emitted | Actor identity |
| `self_ref` | `ActorRef<PoolMsg, R>` | ctor | Self reference — handed to each worker as its `pool_ref` |
| `spawn_fn` | `SpawnFn<R, SpawnRequest<...>>` | ctor, dynamic | Injected worker spawn factory |
| `spawn_ref` | `ActorRef<ChildCtrl<R>, R>` | ctor, dynamic | Supervisor control channel (receives `RegisterDynamicChild`) |
| `notify_ref` | `ActorRef<ChildLifecycleEvent, R>` | ctor, dynamic | Supervisor notification channel for spawned children |
| `spawn_reply_ref` | `ActorRef<SpawnedWorker<...>, R>` | ctor, dynamic | Reply channel the factory sends `SpawnedWorker` to |
| `pending_task_id` | `u32` | state, dynamic | Task ID of the in-flight / most recent spawn |
| `spawn_in_flight` | `bool` | state, dynamic | A spawn request is awaiting its reply |
| `spawn_queue` | `Vec<u32>` | state, dynamic | Buffered `task_id`s received while spawning |
| `worker_refs` | `Vec<ActorRef<WorkerMsg, R>>` | state | Domain refs to spawned workers |
| `worker_ctrls` | `Vec<ActorRef<PeerCtrl<WorkerMsg, R>, R>>` | state | Ctrl refs for peer introduction via `PeerCtrl` |
| `pending` | `u32` | state | Count of outstanding work items |

## Message Contracts

### Receives

| Variant | Mailbox | Payload | Source |
|---------|---------|---------|--------|
| `PoolMsg::SpawnWorker(SpawnWorker { task_id })` | `Msg` | task ID | External (test or app) |
| `PoolMsg::WorkDone(WorkDone { worker_id, task_id, result })` | `Msg` | completion data | Worker actors |
| `PoolEvent::SpawnReply(SpawnedWorker { child_id, domain_ref, ctrl_ref })` | `SpawnReply` (dynamic only) | new worker's refs | Spawn factory (reply to `SpawnRequest`) |

### Sends

| Target | Message | When |
|--------|---------|------|
| Spawn factory | `SpawnRequest::Worker { task_id, reply_to, pool_ref }` | inside `handle_spawn_worker` / `handle_spawned_worker` via `bloxide_spawn::spawn_child` |
| New worker domain ref | `WorkerMsg::DoWork(DoWork { task_id })` | inside `handle_spawned_worker`, after peer introduction |
| Worker ctrl refs (new + existing) | `PeerCtrl::AddPeer(...)` | inside `handle_spawned_worker` via `bloxide_peers::introduce_peers` (bidirectional) |

`SpawnRequest` / `SpawnedWorker` are defined in `blox-ctx-pool-ref` — they carry `ActorRef`s, so they cannot live in the plain-data `pool-messages` crate.

## Entry / Exit Actions

| State | on_entry | on_exit |
|-------|----------|---------|
| `[Init]` (engine) | `on_init`: clear `worker_refs`, `worker_ctrls`, set `pending = 0` | — |
| `Idle` | — | — |
| `Spawning` | — | — |
| `Active` | — | — |

All behavior lives in the transition actions; no state has entry/exit actions.

## Acceptance Criteria

Blox-crate unit tests run against the blox-level **stub** spec (per invariant #18), so
they verify topology and stub-level lifecycle only — with stub actions the ctx fields
never change from their defaults. Behavior that lives in the concrete actions (spawn
accounting, queue processing, `DoWork` dispatch, `WorkDone` accounting) is verified at
the app level by `apps/tokio-pool-demo/tests/`, which drives the system-generated
concrete spec.

- [x] Topology is flat: three leaf states (`Idle`, `Spawning`, `Active`), initial `Idle`, no error state
- [x] `dispatch(PoolEvent::Lifecycle(LifecycleCommand::Start))` exits Init and enters `Idle`
- [x] `PoolMsg::SpawnWorker` in `Idle` transitions to `Spawning` (stub: no accounting at blox level)
- [x] `PoolEvent::SpawnReply` in `Spawning` transitions to `Active` when no spawn is in flight and the queue is empty
- [x] Repeated spawn/reply cycles return to `Active`
- [x] `PoolMsg::WorkDone` in `Active` with `pending == 0` triggers `Decision::Stop` (self-suspend to Init); the machine can be restarted with `Start`
- [x] `PoolMsg::WorkDone` in `Spawning` stays in `Spawning`
- [x] With concrete actions, `SpawnWorker` increments `pending` and `WorkDone` decrements it
- [x] With concrete actions, all work done → `Decision::Stop`
- [x] With concrete actions, spawned workers' refs are stored in `worker_refs` / `worker_ctrls`
- [x] `SpawnWorker` while `Spawning` is buffered in `spawn_queue`
- [x] Queued spawns are processed one reply at a time before returning to `Active`
- [x] A failed `DoWork` send (saturated worker channel) surfaces as `ActionResult::Err`, but the `SpawnReply` guard does not inspect action results — the pool proceeds to `Active` with the work item still owed in `pending`

## Acceptance Criteria → Test Mapping

Blox-level tests live in `crates/bloxes/pool/src/tests.rs` (TestRuntime, stub actions;
run with `cargo test -p pool-blox --features std,dynamic`):

| Acceptance Criterion | Test Function |
|---|---|
| Flat topology, initial state, no error state | `pool_topology_is_flat` |
| `dispatch(LifecycleCommand::Start)` enters Idle | `pool_starts_in_idle` |
| SpawnWorker in Idle → Spawning | `spawn_worker_transitions_idle_to_spawning` |
| SpawnReply in Spawning → Active | `spawn_worker_then_spawned_worker_transitions_to_active` |
| Repeated spawn cycles → Active | `multiple_spawn_workers_stay_active` |
| WorkDone with `pending == 0` → Decision::Stop, restartable | `work_done_with_zero_pending_stops_to_init` |
| WorkDone in Spawning → stay | `work_done_in_spawning_is_absorbed` |

Behavior tests live in `apps/tokio-pool-demo/tests/pool_behavior.rs` (system-level
concrete spec, real actions from `tokio-pool-demo-impl`):

| Acceptance Criterion | Test Function |
|---|---|
| `pending` incremented / decremented | `work_done_decrements_pending` |
| All work done → Decision::Stop | `all_work_done_transitions_to_stop` |
| Worker refs stored | `pool_stores_worker_refs` |
| Failed `DoWork` send keeps `pending` owed | `spawned_worker_with_full_domain_channel_keeps_pending` |
| Spawn while Spawning is queued | `spawn_worker_while_spawning_is_queued` |
| Queued spawns processed after each reply | `queued_spawns_are_processed_after_spawn_reply` |
| WorkDone in Spawning stays Spawning (concrete) | `work_done_in_spawning_state_stays_in_spawning` |
| Full three-worker flow with queue | `full_three_worker_flow_with_queue` |

## Impl Crate Dependencies

All four pool actions are `impl_required = true`; the system-level codegen resolves
them from `tokio-pool-demo-impl` (declared via `impl_crate` in `system.toml`).

| Action function | Operates on | Description |
|-----------------|-------------|-------------|
| `handle_spawn_worker` | `spawn_fn`, `spawn_ref`, `notify_ref`, `spawn_reply_ref`, `pending_task_id`, `spawn_in_flight`, `pending` | Records the task, sets `spawn_in_flight`, `pending += 1`, sends `SpawnRequest::Worker` via `bloxide_spawn::spawn_child` |
| `handle_spawn_worker_queued` | `spawn_queue`, `pending` | Buffers the `task_id`, `pending += 1` |
| `handle_spawned_worker` | spawn fields + `spawn_queue`, `worker_refs`, `worker_ctrls` | Clears `spawn_in_flight`, introduces peers bidirectionally, sends `DoWork`, stores refs, pops the next queued spawn |
| `handle_work_done` | `pending` | `pending -= 1` |

Types: `SpawnRequest` / `SpawnedWorker` from `blox-ctx-pool-ref`; `introduce_peers`
from `bloxide-peers`; `spawn_child` / `SpawnFn` / `SpawnOutput` from `bloxide-spawn`.
(`notify_pool_done` is the *Worker's* dependency on `blox-ctx-pool-ref`, not the Pool's.)

## Implementation Notes

- The `dynamic` feature gates the whole spawn machinery: the `SpawnReply` mailbox, the spawn ctx fields (`spawn_fn`, `spawn_ref`, `notify_ref`, `spawn_reply_ref`, `pending_task_id`, `spawn_in_flight`, `spawn_queue`), the spawn imports, and all `SpawnWorker`/`SpawnReply` transitions are `#[cfg(feature = "dynamic")]`. Without it, the pool handles only `WorkDone` on the `Msg` mailbox.
- Two-phase spawn flow: `SpawnWorker` → (`Idle` or `Active`) `handle_spawn_worker` sends the request → `Spawning`; the factory's `SpawnedWorker` reply → `handle_spawned_worker` (introduce peers, send `DoWork`, process the next queued spawn) → `Active`.
- Queue semantics: at most one spawn is in flight. `SpawnWorker` arriving in `Spawning` is appended to `spawn_queue`; each `SpawnedWorker` reply pops the oldest queued `task_id` and issues its spawn request, so the pool stays in `Spawning` until the queue drains and no spawn is in flight.
- The `SpawnReply` guard does not inspect action results: a failed `DoWork` send surfaces as `ActionResult::Err` but does not divert the transition (see `spawned_worker_with_full_domain_channel_keeps_pending`).
- Pool is generic over `R: BloxRuntime` but requires `SpawnCap` at wiring time (Tokio only); the worker type is abstract — the pool only knows the factory signature.
- See `spec/architecture/10-dynamic-actors.md` for the dynamic spawning pattern.

## Open Questions

None currently.

## Related Docs

- See `spec/bloxes/worker.md` for the worker perspective
- See `apps/tokio-pool-demo/` for the wiring example
