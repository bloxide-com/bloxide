# Blox Spec: `Worker`

## Purpose

The Worker actor demonstrates:
- **Priority mailbox handling**: Ctrl stream polled before domain stream
- **Peer accumulation**: Receives `AddPeer` commands before `DoWork`
- **Result broadcast**: Sends result to all peers before notifying pool
- **Terminal state**: Exits task when work is done

Workers are spawned dynamically by the Pool actor.

## Crate Location

- Blox crate: `crates/bloxes/worker/`
- Messages crate: `crates/messages/pool-messages/` (shared with Pool)
- Actions crate: `crates/actions/pool-actions/` (shared with Pool)
- No impl crate needed — behavior is simple enough for blox-internal state

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Waiting : runtime calls machine.start()
    Waiting --> Done : WorkerMsg::DoWork
```

> `[Init]` is engine-implicit. `Waiting` and `Done` are leaf states.

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for `start()`; `on_init_entry` clears peers and task state |
| `Waiting` | leaf, initial | Accumulating peer introductions; awaiting work assignment |
| `Done` | leaf, terminal | Work complete; broadcast result to peers, notify pool |

## Events

| Event | Handled by | Rule pattern | Guard outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| `WorkerCtrl::AddPeer(_)` | `Waiting` | Action-Then-Stay | `Stay` | `apply_worker_ctrl` |
| `WorkerMsg::DoWork(_)` | `Waiting` | Action-Then-Transition | `Done` | `process_work` |
| `WorkerMsg::PeerResult(_)` | `Waiting` | Sink | `Stay` | none (absorbed) |
| any unhandled | root (no rules) | — | dropped | none |

## Priority Mailbox Ordering

The Worker's `Mailboxes` tuple is ordered for priority poll:

```rust
type Mailboxes<Rt: BloxRuntime> = (R::Stream<WorkerCtrl<R>>, R::Stream<WorkerMsg>);
//                                  ^-- index 0 (ctrl)                     ^-- index 1 (domain)
```

The runtime polls index 0 first, ensuring all `AddPeer` commands arrive before `DoWork`.

> The behavior type parameter `B` must implement `HasWorkerPeers<R>`. This trait provides the concrete peer vector that `WorkerCtrl::AddPeer` appends to.

## Context

```rust
#[derive(BloxCtx)]
pub struct WorkerCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPoolRef<R>)]
    pub pool_ref: ActorRef<PoolMsg, R>,
    #[delegates(HasCurrentTask)]
    pub task_id: u32,
    #[delegates(HasCurrentTask)]
    pub result: u32,
    #[delegates(HasWorkerPeers<R>)]
    pub peers: Vec<ActorRef<WorkerMsg, R>>,
}
```

| Field | Type | Annotation | Description |
|-------|------|------------|-------------|
| `self_id` | `ActorId` | `#[self_id]` | Actor identity |
| `pool_ref` | `ActorRef<PoolMsg, R>` | `#[provides(HasPoolRef<R>)]` | Reference to parent pool |
| `task_id` | `u32` | `#[delegates(HasCurrentTask)]` | Assigned task ID |
| `result` | `u32` | `#[delegates(HasCurrentTask)]` | Computed result |
| `peers` | `Vec<...>` | `#[delegates(HasWorkerPeers<R>)]` | Introduced peer refs |

## Message Contracts

### Receives

| Variant | Stream | Payload | Source |
|---------|--------|---------|--------|
| `WorkerCtrl::AddPeer(ActorRef<WorkerMsg, R>)` | Ctrl (index 0) | peer ref | Pool (via introduce_peers) |
| `WorkerMsg::DoWork(DoWork { task_id })` | Domain (index 1) | task ID | Pool |
| `WorkerMsg::PeerResult(PeerResult { from_id, result })` | Domain (index 1) | peer result | Other workers |

### Sends

| Target | Message | When |
|--------|---------|------|
| All peers | `WorkerMsg::PeerResult(...)` | `Done::on_entry` via `broadcast_to_peers` |
| `pool_ref` | `PoolMsg::WorkDone(...)` | `Done::on_entry` via `notify_pool_done` |

## Entry / Exit Actions

| State | on_entry | on_exit |
|-------|----------|---------|
| `[Init]` (engine) | clear peers, set task_id=0, result=0 | — |
| `Waiting` | `log_waiting` | — |
| `Done` | `log_done`, `broadcast_to_peers`, `notify_pool_done` | — |

## Acceptance Criteria

- [ ] `machine.start()` exits Init and enters `Waiting`
- [ ] `WorkerCtrl::AddPeer` in `Waiting` adds peer to list, stays in `Waiting`
- [ ] `WorkerMsg::DoWork` in `Waiting` sets task_id and result, transitions to `Done`
- [ ] `Done::on_entry` broadcasts result to all accumulated peers
- [ ] `Done::on_entry` sends `WorkDone` to pool
- [ ] `is_terminal(&WorkerState::Done)` returns `true`
- [ ] Ctrl stream is polled with higher priority than domain stream

## Acceptance Criteria → Test Mapping

| Acceptance Criterion | Test Function |
|---|---|
| start enters Waiting | `test_start_enters_waiting()` |
| AddPeer accumulates | `test_add_peer_accumulates()` |
| DoWork transitions to Done | `test_do_work_transitions()` |
| Done broadcasts to peers | `test_done_broadcasts()` |
| Done notifies pool | `test_done_notifies_pool()` |

## Action Crate Dependencies

| Trait | From crate | Implemented by |
|-------|-----------|----------------|
| `HasPoolRef<R>` | `pool-actions` | Generated via `#[provides]` |
| `HasCurrentTask` | `pool-actions` | Generated via `#[delegates]` |
| `HasWorkerPeers<R>` | `pool-actions` | Generated via `#[delegates]` |

## Related Docs

- See `spec/bloxes/pool.md` for the pool perspective
- See `spec/architecture/07-typed-mailboxes.md` for priority ordering
- See `spec/architecture/11-dynamic-actors.md` for peer introduction
