# Blox Spec: `<BloxName>`

> Copy this file to `spec/bloxes/<blox-name>.md` and fill in every section before writing any code.
> Delete instructions in `>` blockquotes as you go.

## Purpose

> One paragraph. What does this actor do? What problem does it solve? What are its responsibilities?

## Crate Location

> Where does this blox live in the workspace?

- Blox crate: `crates/bloxes/<blox-name>/`
- Messages crate: `crates/messages/<blox-name>-messages/` _(if new messages are needed; share with peers using the same protocol)_
- Actions crate: `crates/actions/<blox-name>-actions/` _(accessor/behavior traits + generic action functions; no concrete types)_
- Impl crate: a separate crate consumed by the wiring binary (e.g. `crates/impl/ping-pong-impl/`); contains concrete behavior trait implementations injected into the blox context

## State Hierarchy

> Draw the full state tree. Use `stateDiagram-v2`. Do NOT include Root or Init — both are
> engine-implicit. The `[*] --> FirstState` arrow represents `dispatch(Start)`.
> Composite states wrap their children. Add `note` annotations for important on_entry side effects.

```mermaid
stateDiagram-v2
    [*] --> Idle : dispatch(Start)

    state Operational {
        Idle
        Working
    }

    Idle --> Working : DomainMsg::Begin
    Working --> Idle : DomainMsg::Complete
    Working --> Done : DomainMsg::Finish [guard condition]
```

> Legend:
> - Boxes without children = leaf states (can be active)
> - Boxes with children = composite states (never active; provide shared handlers)
> - Transitions labeled: `MsgType::Variant [guard]`
> - `[Init]` is engine-implicit — never shown in the State enum
> - Lifecycle control (start/reset) is runtime-managed — not shown as transitions

## blox.toml

> The blox.toml file drives code generation via `cargo blox generate`. It declares the event type, state topology, and declarative transitions that the codegen tool turns into Rust source files.

```toml
[actor]
name = "<BloxName>"

[event]
name = "<BloxName>Event"

[[event.mailboxes]]
variant = "Msg"
message = "DomainMsg"
message_path = "domain_messages::DomainMsg"

[context]
name = "<BloxName>Ctx"
generics = "<R: BloxRuntime>"

# Context fields come from [[context.uses]] entries.
# self_id and behavior are auto-emitted by the codegen — do NOT declare them.

[[context.uses]]
crate = "bloxide_messaging"
trait = "HasPeerRef<R, DomainMsg>"
field = "peer_ref"
field_type = "ActorRef<DomainMsg, R>"
role = "accessor"

[topology]

[[topology.states]]
name = "Operational"
composite = true

[[topology.states]]
name = "Idle"
parent = "Operational"
initial = true

[[topology.states]]
name = "Working"
parent = "Operational"

[[topology.states]]
name = "Done"
terminal = true

# --- Declarative transitions ---

# Idle: Begin event → transition to Working
[[topology.transitions]]
state = "Idle"
event = "DomainMsg::Begin(_)"
target = "Working"

# Working: Complete event → action then guard
[[topology.transitions]]
state = "Working"
event = "DomainMsg::Complete(_)"
target = "stay"
actions = ["send_done_to_peer"]

[[topology.transitions.guards]]
condition = "ctx.is_finished()"
target = "Done"

[[topology.transitions.guards]]
condition = "_"
target = "Idle"

# Working: Finish event → transition to Done
[[topology.transitions]]
state = "Working"
event = "DomainMsg::Finish(_)"
target = "Done"

# Entry/exit handlers
[[topology.entry]]
state = "Working"
actions = ["increment_counter"]

[[topology.exit]]
state = "Working"
actions = ["log_work_complete"]
```

> **State declarations** (`[[topology.states]]`): name, parent (optional), composite/initial/terminal/error flags.
>
> **Transition declarations** (`[[topology.transitions]]`):
> - `state` — which state handles this transition (must match a declared state)
> - `event` — match pattern, e.g. `DomainMsg::Begin(_)` or `DomainMsg::A(_) | DomainMsg::B(_)` for or-patterns
> - `target` — `stay` (self-transition), `reset`, `fail`, or a state name (leaf states only)
> - `actions` — list of action function references (e.g. `["Self::log_round", "send_initial_ping"]`)
> - `[[topology.transitions.guards]]` — ordered guard branches; each has `condition` (Rust expression or `_` for catch-all) and `target`
>
> **Entry/exit** (`[[topology.entry]]` / `[[topology.exit]]`):
> - `state` — which state this handler belongs to
> - `actions` — list of `fn(&mut Ctx)` function references
>
> See `spec/architecture/12-action-crate-pattern.md` for the full blox.toml schema.

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for `dispatch(Start)`; `on_init_entry` resets domain state |
| `Operational` | composite | Actor is running; groups Idle and Working |
| `Idle` | leaf | Ready for work |
| `Working` | leaf | Processing a task |
| `Done` | leaf | Terminal state (`is_terminal()` returns `true`); runtime notifies supervisor automatically |

> Adjust table rows to match your state hierarchy diagram exactly.
> Do NOT include a Root row — it is engine-implicit.
> For terminal states, note that `is_terminal()` must be overridden in `MachineSpec`.

## Events

> List every domain event variant this actor's mailbox accepts.
> Do NOT list lifecycle events (start/reset) — those are runtime-managed.
> In the "Rule pattern" column use one of the named patterns from `spec/architecture/05-handler-patterns.md`:
> Pure Transition, Sink, Action-Then-Stay, Action-Then-Guard, Pure Guard, Bubble.

| Event | Handled by | Rule pattern | Guard outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| `DomainMsg::Begin` | `Idle` | Pure Transition | `Guard::Transition(Working)` | none |
| `DomainMsg::Complete` | `Working` | Action-Then-Guard | `Guard::Transition(Idle)` | sends `PeerMsg::Done` |
| `DomainMsg::Finish` | `Working` | Action-Then-Guard | `Guard::Transition(Done)` if guard met, else `Guard::Stay` | none |
| any unhandled | root (no rules) | — | dropped | none |

## Context

> Describe every field in `<BloxName>Ctx<R>`. Use `#[derive(BloxCtx)]` with field annotations.
> See `spec/architecture/12-action-crate-pattern.md` for annotation semantics.
> No `supervisor_ref` field — actors don't hold a reference to their supervisor.

```rust
#[derive(BloxCtx)]
pub struct <BloxName>Ctx<R: BloxRuntime> {
    pub self_id: ActorId,
    // Peer handles (auto-detected from _ref field naming convention):
    pub peer_ref: ActorRef<SharedMsg, R>,
    // Domain state (initialized to Default::default() in generated constructor):
    pub counter: u32,
}
```

Behavior traits (implemented manually, delegating to fields):
- `HasPeerRef<R>` → auto-detected from `peer_ref: ActorRef<SharedMsg, R>` field
- _(list domain behavior traits and their field targets)_

| Field | Type | Detection | Description |
|-------|------|-----------|-------------|
| `self_id` | `ActorId` | Auto-detected `HasSelfId` | Actor identity |
| `peer_ref` | `ActorRef<SharedMsg, R>` | Auto-detected `HasPeerRef<R>` | Handle to peer |
| `counter` | `u32` | _(none — `Default::default()`)_ | Tasks processed |

## Message Contracts

> List every message type this blox sends and receives.
> Use the shared message enum (e.g., `SharedMsg`) for all domain messages.
> The runtime handles lifecycle events (start/reset) — do not list them here.

### Receives (`SharedMsg`)

Defined in `crates/messages/<msg-crate-name>/`.

| Variant | Payload | Sent by |
|---------|---------|---------|
| `SharedMsg::Begin(Begin { data })` | task data | peer actor |

### Sends

| Target | Message | When |
|--------|---------|------|
| `peer_ref` | `SharedMsg::Done(Done { id })` | `Working::on_entry` action |

> The runtime notifies the supervisor of lifecycle events (Started, Done, Reset) automatically.
> Do NOT add supervisor_ref sends here.

## Entry / Exit Actions

> Document non-trivial `on_entry` and `on_exit` behaviors.
> Reference action function names from the actions crate — not closures or inline logic.
> For terminal states, note that on_entry can be empty — runtime handles Done detection.

| State | on_entry | on_exit |
|-------|----------|---------|
| `[Init]` (engine) | reset `counter` to 0 | — |
| `Working` | `increment_counter`, `send_started_to_peer` | — |
| `Done` | _(empty — runtime detects terminal via `is_terminal()`)_ | — |

Each listed action is a free function from the actions crate with signature `fn<C: BehaviorTrait + ...>(&mut C)`. Multiple actions compose via `on_entry: &[action_a, action_b]`.

## Acceptance Criteria

> These become test cases. Each criterion must be verifiable with `StateMachine::dispatch`.

- [ ] `dispatch(LifecycleCommand::Start)` exits Init and enters `Idle`
- [ ] `DomainMsg::Begin` in `Idle` transitions to `Working`
- [ ] `DomainMsg::Complete` in `Working` transitions back to `Idle`
- [ ] `DomainMsg::Finish` in `Working` transitions to `Done` when guard is met
- [ ] `is_terminal(&State::Done)` returns `true`
- [ ] `dispatch(LifecycleCommand::Reset)` from any state exits all states and enters `initial_state()` directly; `on_init_entry` does NOT fire; domain state is reset via `initial_state()::on_entry`
- [ ] `initial_state()::on_entry` does NOT send any messages — domain-state reset only
- [ ] Unknown events bubble to root (no root rules) and are silently dropped

## Action Crate Dependencies

> List the behavior traits from the actions crate that this blox's context must implement.

| Trait | From crate | Implemented by |
|-------|-----------|----------------|
| `HasPeerRef<R>` | `<blox>-actions` | auto-detected from `peer_ref` field |
| `CountsX` | `<blox>-actions` | wraps `counter` field |

## Open Questions

> List any design questions that must be resolved before implementation.

- [ ] Should `Done` be a terminal state or support re-entry via reset?
- [ ] What is the correct mailbox capacity for this actor?
- [ ] Which `bloxide-log` backend does the wiring crate enable?
