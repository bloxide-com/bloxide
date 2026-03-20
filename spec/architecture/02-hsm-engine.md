# HSM Engine

> **When would I use this?** Use this document when implementing `MachineSpec`,
> understanding the dispatch algorithm, Init/start/reset behavior, LCA transitions,
> or the VirtualRoot lifecycle handler. This is the canonical reference for the HSM engine.

The engine lives in `bloxide-core`. It implements hierarchical state machine (HSM) semantics: parent fallback, LCA-based transitions, and run-to-completion dispatch.

## Engine-Implicit Root and Init

Neither `VirtualRoot` nor `Init` appear in the user's `State` enum. Both are engine-managed:

- **VirtualRoot** is implicit. Top-level user states return `None` from `parent()`. The engine prepends VirtualRoot when building state paths for LCA computation — VirtualRoot contains the lifecycle handler table (`root_transitions()` from `MachineSpec`) and intercepts `LifecycleCommand` variants *before* user-declared states see them. If `root_transitions()` returns `&[]`, all lifecycle commands are handled by the engine's default rules: `Start` → exit Init, enter `initial_state()`; `Reset` → full LCA exit chain to Init; `Stop` → exit to Init, then signal task termination; `Ping` → respond with `ChildLifecycleEvent::Alive`..

- **Init** is implicit. Construction is **silent** — no callbacks fire. The machine starts in `Init` and waits for a `LifecycleCommand::Start` event to be dispatched. `on_init_entry` fires when the machine re-enters `Init` after a Reset — it is for resetting domain state (counters, timers, etc.) only. All non-lifecycle events dispatched while in `Init` are **silently dropped**. Lifecycle commands are handled at VirtualRoot level, so the machine in Init still processes Start/Reset/Stop/Ping via the engine's lifecycle handler.

## State Hierarchy Concept

States form a tree. Only **leaf states** (states with no children) may be active. Composite (non-leaf) states exist solely to group children and provide shared transition rules for implicit bubbling.

```mermaid
flowchart TD
    VR["[VirtualRoot — engine implicit]"]
    Init["[Init — engine implicit]"]
    Active
    Done

    VR --> Init
    VR --> Active
    VR --> Done
```

> This is the `PingState` topology (simplified). `VirtualRoot` and `Init` are engine-implicit — not in the user's `State` enum. `Active` and `Done` are user-declared leaf states. Lifecycle commands (Start, Reset, Stop, Ping) are matched against VirtualRoot's transition rules *first*, before any user-declared state sees them.

A deeper example showing nested composite states:

```mermaid
flowchart TD
    VR["[VirtualRoot — engine implicit]"]
    Init["[Init — engine implicit]"]
    Operational
    Idle
    Running
    Connecting
    Connected

    VR --> Init
    VR --> Operational
    Operational --> Idle
    Operational --> Running
    Running --> Connecting
    Running --> Connected
```

Events bubble up from the active leaf through each ancestor until one handles it, or VirtualRoot catches it. Lifecycle commands are special: VirtualRoot intercepts them regardless of current state (including Init) and applies the appropriate lifecycle transition.

## Core API

### `MachineSpec` trait (`spec.rs`)

```rust
pub trait MachineSpec: Sized + 'static {
    type State: StateTopology;
    type Event: EventTag + Send + 'static;
    type Ctx: 'static;
    type Mailboxes<R: BloxRuntime>: Mailboxes<Self::Event>;

    const HANDLER_TABLE: &'static [&'static StateFns<Self>];

    // First operational leaf state entered after start():
    fn initial_state() -> Self::State;

    // Called when machine re-enters Init after reset() — domain-state reset only:
    fn on_init_entry(ctx: &mut Self::Ctx);

    // Optional: called when leaving Init (just before entering initial_state()):
    fn on_init_exit(_ctx: &mut Self::Ctx) {}

    // Returns true if state is terminal (runtime emits ChildLifecycleEvent::Done):
    fn is_terminal(_state: &Self::State) -> bool { false }

    // Returns true if state is an error state (runtime emits ChildLifecycleEvent::Failed).
    // is_error takes precedence over is_terminal — if both return true, only Failed is emitted:
    fn is_error(_state: &Self::State) -> bool { false }


    // Root-level rules (lifecycle + domain fallback). Empty for supervised actors (lifecycle handled by VirtualRoot defaults). Override to add domain-specific root rules or custom lifecycle behavior.:
    fn root_transitions() -> &'static [StateRule<Self>] { &[] }
}
```

### `MachineSpec` quick map: `State` -> `StateFns` -> `HANDLER_TABLE`

For most bloxes, the mapping is generated from the state enum declaration order:

```rust
#[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
#[handler_fns(READY_FNS, DONE_FNS)]
pub enum CounterState {
    Ready,
    Done,
}

impl<R: BloxRuntime, B: CountsTicks + 'static> MachineSpec for CounterSpec<R, B> {
    // ...
    const HANDLER_TABLE: &'static [&'static StateFns<Self>] =
        counter_state_handler_table!(Self);
}
```

`state.as_index()` is used to index `HANDLER_TABLE`, so variant order and handler-table
order must stay aligned. Using the generated `*_state_handler_table!(Self)` macro avoids
manual ordering mistakes. See the API docs in
`crates/bloxide-core/src/spec.rs` (`MachineSpec::HANDLER_TABLE`) for details.

### `StateMachine` — runtime-facing methods

```rust
impl<S: MachineSpec> StateMachine<S> {
    /// Construct silently in Init. No callbacks fire.
    pub fn new(ctx: S::Ctx) -> Self;

    /// Dispatch an event (domain or lifecycle). All events, including
    /// LifecycleCommand variants (Start, Reset, Stop, Ping), flow through
    /// this method. VirtualRoot intercepts lifecycle commands before user states.
    /// 
    /// Lifecycle outcomes:
    /// - Start (from Init) → Started(initial_state)
    /// - Start (already operational) → HandledNoTransition (idempotent)
    /// - Reset → Reset (full LCA exit chain, task stays alive)
    /// - Stop → Reset + task exit signal
    /// - Ping → HandledNoTransition (emits Alive event)
    pub fn dispatch(&mut self, event: S::Event) -> DispatchOutcome<S::State>;

    /// Shared reference to the machine context.
    pub fn ctx(&self) -> &S::Ctx;

    /// Mutable reference to the machine context.
    pub fn ctx_mut(&mut self) -> &mut S::Ctx;

    /// Current operational leaf state, or None if in Init.
    pub fn current_state(&self) -> Option<S::State>;
}
```

> **Note**: The `start()` and `reset()` methods are deprecated. Lifecycle is driven
> entirely through `dispatch()` with LifecycleCommand events. VirtualRoot handles
> these commands internally, and the runtime observes DispatchOutcome to emit
> ChildLifecycleEvents to supervisors.

### `DispatchOutcome`

```rust
pub enum DispatchOutcome<State> {
    /// No rule matched anywhere (event bubbled to VirtualRoot with no match).
    NoRuleMatched,
    /// Rule matched but guard returned Stay.
    HandledNoTransition,
    /// Transition occurred to a user state.
    Transition(MachineState<State>),
    /// Left Init via Start command.
    Started(MachineState<State>),
    /// Transitioned to terminal state.
    Done(MachineState<State>),
    /// Actor reset to Init via Guard::Reset.
    Reset,
    /// Actor failed to Init via Guard::Fail or entered error state.
    Failed,
    /// Actor stopped to Init via LifecycleCommand::Stop.
    Stopped,
    /// Actor responded to Ping.
    Alive,
}
```

### Lifecycle Command Handling at VirtualRoot

Lifecycle commands are detected via `event.as_lifecycle_command()` and handled *before* any state handler sees them:

| Command | Behavior | DispatchOutcome |
|---------|----------|-----------------|
| `Start` | If in Init: exit Init, enter `initial_state()` | `Started(MachineState::State(state))` |
| `Start` | If already operational: no-op (idempotent) | `HandledNoTransition` |
| `Reset` | Full LCA exit chain → enter `initial_state()` (immediately operational) | `Started(MachineState::State(state))` |
| `Stop` | Full LCA exit chain → Init (suspended, can be restarted) | `Stopped` |
| `Ping` | Respond with health notification | `Alive` |
| `Kill` | Task aborted immediately (permanently dead) | `Failed` |

The runtime inspects `DispatchOutcome` after every call to generate `ChildLifecycleEvent` for the supervisor:
- `Started(s)` or `Transition(s)` where `is_error(&s)` → emits `ChildLifecycleEvent::Failed` (`is_error` takes precedence over `is_terminal`)
- `Started(s)` or `Transition(s)` where `is_terminal(&s)` → emits `ChildLifecycleEvent::Done`
- `Done(s)` → emits `ChildLifecycleEvent::Done`
- `Failed` → emits `ChildLifecycleEvent::Failed`
- `Started(s)` → emits `ChildLifecycleEvent::Started`
- `Reset` → emits `ChildLifecycleEvent::Reset`
- `Stopped` → emits `ChildLifecycleEvent::Stopped`
- `Alive` → emits `ChildLifecycleEvent::Alive`
- `NoRuleMatched`, `HandledNoTransition` → no supervisor notification

### `StateFns` — handler table for one state

```rust
pub struct StateFns<S: MachineSpec + 'static> {
    pub on_entry:    &'static [fn(&mut S::Ctx)],
    pub on_exit:     &'static [fn(&mut S::Ctx)],
    pub transitions: &'static [StateRule<S>],
}
```

All function pointers are static (`fn`, not `dyn Fn`). All mutable state lives in `Ctx`. The `transitions` slice is evaluated in declaration order; the first matching rule wins. **Bubbling is implicit**: if no rule matches in the current state, the engine moves the cursor to the parent and evaluates that state's rules. No manual "return Parent" — bubbling happens automatically when no rule matches.

`on_entry` and `on_exit` are slices — multiple actions compose by listing them: `on_entry: &[increment_round, send_initial_ping]`.

### `StateRule` and `Guard`

```rust
pub struct TransitionRule<S: MachineSpec, G> {
    pub event_tag: u8,
    pub matches:  fn(&S::Event) -> bool,
    pub actions:  &'static [fn(&mut S::Ctx, &S::Event) -> ActionResult],
    pub guard:    fn(&S::Ctx, &ActionResults, &S::Event) -> G,
}
```

> **`ActionResult` vs `ActionResults`**: Each action returns `ActionResult` (Ok/Err). The engine collects all action results into `ActionResults` before calling the guard. Guards receive `&ActionResults` to inspect failures (e.g. `results.any_failed()`) and decide the transition.

```rust
pub type StateRule<S> = TransitionRule<S, Guard<S>>;

pub enum Guard<S: MachineSpec> {
    Transition(LeafState<S::State>),
    Stay,
    Reset,  // exits entire operational chain and re-enters Init
}
```

> `LeafState<S::State>` is a newtype that `debug_assert!`s the target is a leaf state at construction. The `transitions!` proc macro wraps targets in `LeafState::new` automatically — user-facing syntax is unchanged.

### Root Rules

Root rules use the same `StateRule<S>` type as state-level rules — `root_transitions()` returns `&'static [StateRule<Self>]`. Both state-level and root-level rules have access to `Transition`, `Stay`, and `Reset` via `Guard<S>`. There is no separate `RootRule` type in the codebase; the `root_transitions!` macro generates `StateRule` items, identical to what `transitions!` generates.

Root rules are evaluated when an event bubbles past all user-declared ancestor states. Most actors leave `root_transitions()` at its default `&[]` — unhandled events are silently dropped. Since `Guard::Reset` is available in any transition rule (state-level or root-level), actors can self-terminate from any handler without needing root rules.

## Operational Dispatch Algorithm

```mermaid
flowchart TD
    A([Event from mailbox]) --> B{in_init?}
    B -->|"yes"| DROP([Drop silently])
    B -->|"no"| C["cursor = Some(current_state)"]
    C --> D{"cursor = Some(state)?"}

    D -->|"yes"| E["Iterate handlers(state).transitions in order"]
    E --> F{"rule.matches(&event)?"}
    F -->|"false"| G["next rule / no more rules"]
    G --> H{"more rules?"}
    H -->|"yes"| F
    H -->|"no"| I["cursor = parent(state) — implicit bubble"]
    I --> D

    F -->|"true"| J["run actions\nguard = rule.guard(ctx, results, event)"]
    J --> K{guard?}
    K -->|"Transition(target)"| L[change_state]
    K -->|Stay| M([Return DispatchOutcome::HandledNoTransition])
    K -->|Reset| R["enter_init():\nexit all states leaf-first\non_init_entry()\nReturn DispatchOutcome::Reset"]

    D -->|"None (no parent)"| N["Iterate root_transitions() in order"]
    N --> O{"root rule matches?"}
    O -->|"no more rules"| NRM([Return DispatchOutcome::NoRuleMatched])
    O -->|"yes"| P["run actions then guard"]
    P --> Q{Guard?}
    Q -->|"Transition(target)"| L
    Q -->|Stay| M
    Q -->|Reset| R

    L --> TRANS([Return DispatchOutcome::Transition])
```

**Run-to-completion**: the entire dispatch loop runs to `Return` before the actor consumes the next mailbox message.

## LCA Transition Algorithm

When `change_state(source, target)` is called:

```mermaid
flowchart TD
    A["Build source_path:\nwalk parent() from source up\nthen reverse to root-first order"] --> B
    B["Build target_path:\nwalk parent() from target up\nthen reverse"] --> C
    C["Find LCA:\nlast i where source_path[i] == target_path[i]\nNone if no common prefix"] --> D
    D{"LCA?"}
    D -->|"Some(i)"| E["Exit source_path[i+1..] leaf-first\nEnter target_path[i+1..] root-first"]
    D -->|None| F["Exit all of source_path leaf-first\nEnter all of target_path root-first"]
    E --> G["current_state = target"]
    F --> G
```

`LCA = None` occurs when source and target are in different top-level subtrees (no shared user ancestor). The engine exits everything from source up to the virtual root, then enters everything from the virtual root down to target.

### Exit/Entry ordering example

Transition from `Connected` → `Idle` in the deep hierarchy above:

```
source_path (root-first): [Operational, Running, Connected]
target_path (root-first): [Operational, Idle]
LCA = Operational (index 0)

Exit (leaf → LCA, not including LCA):
  Connected.on_exit
  Running.on_exit

Entry (LCA child → target, including target):
  Idle.on_entry
```

### Cross-subtree transition (LCA = None)

Transition from `Idle` → `Active` when they are in different top-level subtrees:

```
source_path: [OldGroup, Idle]
target_path: [NewGroup, Active]
LCA = None (no common prefix)

Exit all source: Idle.on_exit, OldGroup.on_exit
Enter all target: NewGroup.on_entry, Active.on_entry
```

### Stay vs self-transition

- **`Guard::Stay`** — the machine remains in the current state. No `on_exit` or `on_entry` fires. Use when a rule handles an event with side effects but no state change.
- **`Transition(current_state)`** (self-transition) — the LCA is forced to the **virtual parent** of the current state. If the state is top-level (no user parent), LCA = None, causing full exit + re-entry. Use when you need `on_exit` and `on_entry` to fire (e.g. retry loops that reset state on entry).

## `StateMachine` construction and Init

```rust
let machine = StateMachine::new(ctx);
// Construction is silent: no callbacks fire. Machine is in Init.
// on_init_entry does NOT fire here.
// The runtime calls machine.start() when it receives LifecycleCommand::Start.
```

**Init semantics:**
- `new(ctx)` — machine enters Init silently. No `on_init_entry` fires.
- `start()` — exits Init, enters `initial_state()`. Returns `Started(state)`. If already operational, returns `HandledNoTransition` (idempotent).
- `reset()` — exits all operational states leaf-first, calls `on_init_entry`, sets phase to Init. Returns `Reset`. If already in Init, returns `InitNoOp` (idempotent).

## Reset Semantics

Both `machine.reset()` (runtime-initiated) and `Guard::Reset` (returned by any transition guard) invoke the same `enter_init()` engine method. The engine:

1. Exits the current leaf state (`on_exit` for each action in the slice)
2. Exits every ancestor up to the virtual root (`on_exit` for each)
3. Calls `on_init_entry` — for domain-state reset only (counters, cancel timers, etc.)
4. Sets phase to `Init`
5. Returns `DispatchOutcome::Reset`

**The full LCA exit chain is absolute.** Neither `machine.reset()` nor `Guard::Reset` skips any `on_exit` handler — every state from the current leaf up to the topmost ancestor fires its exit actions before `on_init_entry` runs.

**Two paths to Reset, identical behavior:**

- **Runtime-initiated**: the runtime calls `machine.reset()` in response to `LifecycleCommand::Reset`. This is how supervisors reset children.
- **Self-initiated**: a transition guard at any level (state or root) returns `Guard::Reset`. This is how actors self-terminate in response to domain events (e.g., a supervisor resetting itself after all children have shut down).

**Reset is valid from any operational state.** `on_exit` handlers must be safe to call unconditionally.

Example — Reset while in `Paused` (child of `Operating`):

```
current_state = Paused
source_path = [Operating, Paused]

Reset exits:
  Paused.on_exit      ← must handle "timer may not be running"
  Operating.on_exit   ← must handle "counter may be 0"
Then:
  on_init_entry()     ← resets round counter; no supervisor notification needed
```

## `is_terminal` and Done Detection

Actors with terminal states override `is_terminal`:

```rust
fn is_terminal(state: &PingState) -> bool {
    matches!(state, PingState::Done)
}
```

The runtime checks `is_terminal` after `DispatchOutcome::Started(s)` and `DispatchOutcome::Transition(s)`. If it returns `true`, the runtime emits `ChildLifecycleEvent::Done { child_id }` to the supervisor. The actor itself does nothing special in `Done::on_entry` — no supervisor notification required.

## Topology Invariants

The `parent()` function must form a **tree**:

1. Every chain of `parent()` calls from any state must terminate at `None` (no cycles).
2. Two root-first paths from any pair of states either share a monotone common prefix and then diverge, or share no prefix at all (no DAG re-convergence).
3. `parent()` returns `None` only for top-level states (those that are direct children of the virtual root). There can be multiple top-level states.

The `find_lca` algorithm relies on invariant (2). A `debug_assert!` in the engine detects DAG topologies in debug builds. The recommended verification test:

```rust
#[test]
fn test_topology_no_cycles() {
    use std::collections::HashSet;
    for &s in &ALL_STATES {
        let mut seen = HashSet::new();
        let mut cursor = Some(s);
        while let Some(c) = cursor {
            assert!(seen.insert(c), "cycle at {:?}", c);
            cursor = MySpec::parent(c);
        }
    }
}
```

## Related Docs

- **Dispatch semantics** → This file
- **State topology definition** → `crates/bloxide-core/src/topology.rs`
- **MachineSpec trait** → `crates/bloxide-core/src/spec.rs`
- **Handler patterns** → `spec/architecture/05-handler-patterns.md`
- **Transitions macro** → `skills/building-with-bloxide/reference.md` → `transitions!`
- **Init/Start/Reset flow** → `START_HERE.md` → "Init, Start, and Reset"
