# Blox Spec: `Pool`

## Purpose

The Pool actor demonstrates dynamic actor spawning and peer introduction:
- Receives `SpawnWorker` commands to create workers at runtime
- Tracks spawned workers and their completion status
- Introduces new workers to existing peers via `WorkerCtrl`
- Reaches terminal state when all workers report done

This blox showcases:
- **Factory injection**: Worker spawn function injected at wiring time
- **Peer introduction**: Using `bloxide-spawn` to wire workers together
- **Dynamic actor creation**: Tokio runtime's `SpawnCap` capability

## Crate Location

- Blox crate: `crates/bloxes/pool/`
- Messages crate: `crates/messages/pool-messages/`
- Actions crate: `crates/actions/pool-actions/`
- Impl crate: `crates/impl/tokio-pool-demo-impl/` (provides worker factory)

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Idle : runtime calls machine.start()
    Idle --> Active : PoolMsg::SpawnWorker
    Active --> Active : PoolMsg::SpawnWorker
    Active --> AllDone : PoolMsg::WorkDone [pending == 0]
```

> `[Init]` is engine-implicit. `Idle`, `Active`, `AllDone` are leaf states.

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for `start()`; `on_init_entry` clears worker lists |
| `Idle` | leaf, initial | No workers spawned yet |
| `Active` | leaf | At least one worker running; accepts more spawns and work done |
| `AllDone` | leaf, terminal | All workers finished; `is_terminal()` returns `true` |

## Events

| Event | Handled by | Rule pattern | Guard outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| `PoolMsg::SpawnWorker(task_id)` | `Idle` | Action-Then-Transition | `Active` | `spawn_worker`, `introduce_new_worker` |
| `PoolMsg::SpawnWorker(task_id)` | `Active` | Action-Then-Stay | `Stay` | `spawn_worker`, `introduce_new_worker` |
| `PoolMsg::WorkDone(_)` | `Active` | Action-Then-Guard | `AllDone` if pending==0, else `Stay` | decrement pending count |
| any unhandled | root (no rules) | — | dropped | none |

## Context

```rust
#[derive(BloxCtx)]
pub struct PoolCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasSelfRef<R>)]
    pub self_ref: ActorRef<PoolMsg, R>,
    #[provides(HasWorkerFactory<R>)]
    pub worker_factory: WorkerSpawnFn<R>,
    #[ctor]
    pub worker_refs: Vec<ActorRef<WorkerMsg, R>>,
    #[ctor]
    pub worker_ctrls: Vec<ActorRef<WorkerCtrl<R>, R>>,
    #[ctor]
    pub pending: u32,
}
```

| Field | Type | Annotation | Description |
|-------|------|------------|-------------|
| `self_id` | `ActorId` | `#[self_id]` | Actor identity |
| `self_ref` | `ActorRef<PoolMsg, R>` | `#[provides(HasSelfRef<R>)]` | Self reference (for worker callbacks) |
| `worker_factory` | `WorkerSpawnFn<R>` | `#[provides(HasWorkerFactory<R>)]` | Injected spawn function |
| `worker_refs` | `Vec<...>` | `#[ctor]` | Domain refs to spawned workers |
| `worker_ctrls` | `Vec<ActorRef<WorkerCtrl<R>, R>>` | `#[ctor]` | Ctrl refs for peer introduction via `WorkerCtrl` |
| `pending` | `u32` | `#[ctor]` | Count of running workers |

## Message Contracts

### Receives (`PoolMsg`)

| Variant | Payload | Source |
|---------|---------|--------|
| `PoolMsg::SpawnWorker(SpawnWorker { task_id })` | task ID | External (test or app) |
| `PoolMsg::WorkDone(WorkDone { worker_id, task_id, result })` | completion data | Worker actors |

### Sends

| Target | Message | When |
|--------|---------|------|
| New worker domain ref | `WorkerMsg::DoWork(DoWork { task_id })` | After spawn and peer introduction |
| Worker ctrl refs | `WorkerCtrl::AddPeer(...)` | Via `introduce_new_worker` |

## Entry / Exit Actions

| State | on_entry | on_exit |
|-------|----------|---------|
| `[Init]` (engine) | clear worker_refs, worker_ctrls, set pending=0 | — |
| `Idle` | — | — |
| `Active` | — | — |
| `AllDone` | `log_all_done` | — |

## Acceptance Criteria

- [ ] `machine.start()` exits Init and enters `Idle`
- [ ] `PoolMsg::SpawnWorker` in `Idle` spawns worker, transitions to `Active`
- [ ] `PoolMsg::SpawnWorker` in `Active` spawns additional worker, stays in `Active`
- [ ] New worker is introduced to all previously spawned workers
- [ ] `PoolMsg::WorkDone` in `Active` decrements pending count
- [ ] `PoolMsg::WorkDone` in `Active` with `pending == 0` transitions to `AllDone`
- [ ] `is_terminal(&PoolState::AllDone)` returns `true`
- [ ] `machine.reset()` from `AllDone` clears all worker tracking state

## Acceptance Criteria → Test Mapping

| Acceptance Criterion | Test Function |
|---|---|
| start enters Idle | `test_start_enters_idle()` |
| SpawnWorker in Idle → Active | `test_first_spawn_activates()` |
| SpawnWorker in Active stays | `test_additional_spawn_stays()` |
| WorkDone decrements pending | `test_work_done_decrements()` |
| All workers done → AllDone | `test_all_workers_done()` |

## Action Crate Dependencies

| Trait | From crate | Implemented by |
|-------|-----------|----------------|
| `HasWorkerFactory<R>` | `pool-actions` | Generated via `#[provides]` |
| `HasWorkers<R>` | `pool-actions` | Manual accessor methods |
| `HasSelfRef<R>` | `pool-actions` | Generated via `#[provides]` |

## Implementation Notes

- The factory pattern remains unchanged: `worker_factory: WorkerSpawnFn<R>` is injected at wiring time
- Pool uses `WorkerCtrl` for peer introduction between workers
- Pool is generic over `R: BloxRuntime` but requires `SpawnCap` at wiring time (Tokio only)
- Worker type is abstract — pool only knows the factory signature
- See `spec/architecture/11-dynamic-actors.md` for the dynamic spawning pattern

## Related Docs

- See `spec/bloxes/worker.md` for the worker perspective
- See `tokio-pool-demo.rs` for wiring example
