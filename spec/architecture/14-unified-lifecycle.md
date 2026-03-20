# Unified Lifecycle Architecture

This document specifies the unified lifecycle model for Bloxide actors. This is a significant architectural change from the previous dual-track system.

## Design Decisions Summary

| Decision | Choice |
|----------|--------|
| Init state | Implicit leaf, not in user enum, tracked via `MachineState<S>` type |
| Lifecycle commands | Start/Reset/Stop flow through dispatch, handled at VirtualRoot |
| Kill | Runtime capability (not a message), supervisor calls `runtime.kill(actor_id)` |
| Error recovery | `Guard::Fail` → transition to Init → supervisor sees `Failed` |
| Init on_entry | Fires when entering Init (Reset/Fail/Stop), NOT at construction |
| Init transitions | Auto-generated catch-all: all domain events → stay |
| Supervisor ref | Not needed for lifecycle; optional for domain notifications |
| MachineState type | `enum MachineState<S> { Init, State(S) }` - like Option but semantically clear |

---

## Phase 1: New Type Definitions

### 1.1 MachineState Type

**File:** `crates/bloxide-core/src/engine.rs` (or new `state.rs`)

```rust
/// Represents the current state of a machine, including the implicit Init.
/// 
/// Init is implicit (not part of the user's state enum) and tracked separately.
/// Users may have their own domain state also named "Init" with no conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineState<S> {
    /// Implicit Init state - machine is in lifecycle wait state.
    Init,
    /// One of the user's declared operational states.
    State(S),
}

impl<S> MachineState<S> {
    pub fn is_init(&self) -> bool {
        matches!(self, MachineState::Init)
    }
    
    pub fn as_state(&self) -> Option<&S> {
        match self {
            MachineState::Init => None,
            MachineState::State(s) => Some(s),
        }
    }
    
    /// Returns the state if present, or a default. Used for pattern matching.
    pub fn unwrap_state(self) -> S 
    where S: Default 
    {
        match self {
            MachineState::Init => S::default(),
            MachineState::State(s) => s,
        }
    }
}
```

### 1.2 Guard::Fail Variant

**File:** `crates/bloxide-core/src/transition.rs`

```rust
pub enum Guard<S: MachineSpec> {
    /// Transition to target state.
    Transition(LeafState<S::State>),
    /// Stay in current state.
    Stay,
    /// Exit to implicit Init, report Reset to supervisor.
    Reset,
    /// Exit to implicit Init, report Failed to supervisor.
    Fail,
}
```

### 1.3 Updated DispatchOutcome

**File:** `crates/bloxide-core/src/engine.rs`

```rust
/// Outcome of dispatching an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome<S> {
    /// No rule matched anywhere (event bubbled to VirtualRoot with no match).
    NoRuleMatched,
    /// Rule matched but guard returned Stay.
    HandledNoTransition,
    /// Transition occurred to a user state.
    Transition(MachineState<S>),
    /// Left Init via Start command.
    Started(MachineState<S>),
    /// Transitioned to terminal state.
    Done(MachineState<S>),
    /// Actor reset to Init via Guard::Reset.
    Reset,
    /// Actor failed to Init via Guard::Fail or entered error state.
    Failed,
    /// Actor stopped to Init via LifecycleCommand::Stop.
    Stopped,
    /// Actor responded to Ping.
    Alive,
}

// Remove: DroppedInInit, InitNoOp (no longer needed)
```

---

## Phase 2: Core Engine Refactor

### 2.1 StateMachine Struct Update

**File:** `crates/bloxide-core/src/engine.rs`

```rust
pub struct StateMachine<S: MachineSpec> {
    /// Current state - either implicit Init or a user state.
    current: MachineState<S::State>,
    /// Actor context.
    ctx: S::Ctx,
}

impl<S: MachineSpec> StateMachine<S> {
    /// Construct a new machine in implicit Init state.
    /// 
    /// Init is entered SILENTLY - no callbacks fire. Construction is just
    /// setting the initial state. `on_init_entry` only fires when entering
    /// Init due to Reset/Fail/Stop.
    pub fn new(ctx: S::Ctx) -> Self {
        Self {
            current: MachineState::Init,
            ctx,
        }
    }
    
    /// Current state of the machine.
    pub fn current_state(&self) -> MachineState<S::State> {
        self.current
    }
    
    /// Mutable reference to context.
    pub fn ctx_mut(&mut self) -> &mut S::Ctx {
        &mut self.ctx
    }
}
```

### 2.2 Dispatch Implementation

**File:** `crates/bloxide-core/src/engine.rs`

```rust
impl<S: MachineSpec> StateMachine<S> {
    /// Dispatch an event through the state machine.
    /// 
    /// Lifecycle commands (Start/Reset/Stop/Ping) are handled at VirtualRoot.
    /// Domain events flow through state handler tables, bubbling to root.
    pub fn dispatch(&mut self, event: S::Event) -> DispatchOutcome<S::State> {
        // Check for lifecycle commands first (VirtualRoot handling)
        if let Some(cmd) = event.as_lifecycle_command() {
            return self.handle_lifecycle(cmd);
        }
        
        // Domain event flow depends on current state
        match self.current {
            MachineState::Init => {
                // Init's auto-generated transitions catch all domain events
                self.process_init_event(event)
            }
            MachineState::State(_) => {
                self.process_operational_event(event)
            }
        }
    }
    
    /// Handle lifecycle commands at VirtualRoot level.
    fn handle_lifecycle(&mut self, cmd: LifecycleCommand) -> DispatchOutcome<S::State> {
        match cmd {
            LifecycleCommand::Start => {
                // Transition from Init to user's initial state
                let target = S::initial_state();
                self.transition_to_state(target);
                DispatchOutcome::Started(MachineState::State(target))
            }
            LifecycleCommand::Reset => {
                // Transition to Init, report Reset
                self.transition_to_init();
                DispatchOutcome::Reset
            }
            LifecycleCommand::Stop => {
                // Transition to Init, report Stopped
                self.transition_to_init();
                DispatchOutcome::Stopped
            }
            LifecycleCommand::Ping => {
                // Respond Alive (runtime will send notification)
                DispatchOutcome::Alive
            }
        }
    }
    
    /// Process event while in implicit Init (catch-all behavior).
    fn process_init_event(&mut self, _event: S::Event) -> DispatchOutcome<S::State> {
        // Init's auto-generated transitions: all domain events => stay
        // No callbacks, no state change
        DispatchOutcome::HandledNoTransition
    }
    
    /// Process event while in operational state.
    fn process_operational_event(&mut self, event: S::Event) -> DispatchOutcome<S::State> {
        let current = match self.current {
            MachineState::State(s) => s,
            MachineState::Init => unreachable!("process_operational_event called while in Init"),
        };
        let event_tag = event.event_tag();
        let current_path = current.path();
        
        // Walk from leaf to root, evaluating state handlers
        for &ancestor in current_path.iter().rev() {
            let fns = Self::handler_fns_for_state(ancestor);
            
            if let Some(guard) = eval_rules(fns.transitions, &mut self.ctx, &event, event_tag) {
                return self.apply_guard(guard);
            }
        }
        
        // Bubbled to VirtualRoot - no match in any state
        DispatchOutcome::NoRuleMatched
    }
    
    /// Apply a Guard outcome.
    fn apply_guard(&mut self, guard: Guard<S>) -> DispatchOutcome<S::State> {
        match guard {
            Guard::Transition(leaf) => {
                let target = leaf.into_inner();
                self.transition_to_state(target);
                
                // Check for terminal/error states
                if S::is_error(&target) {
                    DispatchOutcome::Failed
                } else if S::is_terminal(&target) {
                    DispatchOutcome::Done(MachineState::State(target))
                } else {
                    DispatchOutcome::Transition(MachineState::State(target))
                }
            }
            Guard::Stay => DispatchOutcome::HandledNoTransition,
            Guard::Reset => {
                self.transition_to_init();
                DispatchOutcome::Reset
            }
            Guard::Fail => {
                self.transition_to_init();
                DispatchOutcome::Failed
            }
        }
    }
    
    /// Transition to a user state (with LCA exit/entry callbacks).
    fn transition_to_state(&mut self, target: S::State) {
        match self.current {
            MachineState::Init => {
                // Exiting Init: fire on_init_exit, then enter target states
                S::on_init_exit(&mut self.ctx);
                let path = target.path();
                for &state in path.iter() {
                    for action in Self::handler_fns_for_state(state).on_entry {
                        action(&mut self.ctx);
                    }
                }
                self.current = MachineState::State(target);
            }
            MachineState::State(source) => {
                // Normal state-to-state transition with LCA
                self.change_state(source, target);
            }
        }
    }
    
    /// Transition to implicit Init (with LCA exit callbacks).
    fn transition_to_init(&mut self) {
        match self.current {
            MachineState::Init => {
                // Already in Init, nothing to do
            }
            MachineState::State(source) => {
                // Exit all states leaf-to-root
                let source_path = source.path();
                for &state in source_path.iter().rev() {
                    for action in Self::handler_fns_for_state(state).on_exit {
                        action(&mut self.ctx);
                    }
                }
                // Enter Init: fire on_init_entry
                S::on_init_entry(&mut self.ctx);
                self.current = MachineState::Init;
            }
        }
    }
    
    /// Get handler functions for a user state.
    fn handler_fns_for_state(state: S::State) -> &'static StateFns<S> {
        S::HANDLER_TABLE[state.as_index()]
    }
}
```

### 2.3 Remove Old Methods

**File:** `crates/bloxide-core/src/engine.rs`

Remove:
- `pub fn start(&mut self) -> DispatchOutcome<S::State>`
- `pub fn reset(&mut self) -> DispatchOutcome<S::State>`
- `fn leave_init(&mut self)` (replaced by `transition_to_state`)
- `fn enter_init(&mut self)` (replaced by `transition_to_init`)
- `MachinePhase` enum entirely

---

## Phase 3: MachineSpec Trait Updates

**File:** `crates/bloxide-core/src/spec.rs`

```rust
pub trait MachineSpec: Sized + 'static {
    type State: StateTopology;
    type Event: EventTag + Send + 'static;
    type Ctx: 'static;
    type Mailboxes<R: BloxRuntime>: Mailboxes<Self::Event>;

    const HANDLER_TABLE: &'static [&'static StateFns<Self>];

    /// The initial operational state to transition to on Start command.
    /// MUST return a leaf state (not a composite state).
    fn initial_state() -> Self::State;

    /// Called when entering implicit Init via Reset/Fail/Stop.
    /// Use for cleanup, resource release, state reset.
    /// NOT called at construction time.
    fn on_init_entry(_ctx: &mut Self::Ctx) {}

    /// Called when leaving implicit Init via Start command.
    /// Use for setup, initialization before entering operational states.
    fn on_init_exit(_ctx: &mut Self::Ctx) {}

    /// Returns true if state represents normal completion.
    /// Transitions to this state report Done to supervisor.
    fn is_terminal(_state: &Self::State) -> bool { false }

    /// Returns true if state represents a failure.
    /// Transitions to this state report Failed to supervisor.
    /// Takes precedence over is_terminal if both true.
    fn is_error(_state: &Self::State) -> bool { false }

    // REMOVED: fn is_start(_event: &Self::Event) -> bool { false }
    // REMOVED: fn root_transitions() -> &'static [StateRule<Self>] { &[] }
}
```

---

## Phase 4: Lifecycle Event Integration

### 4.1 Event Trait for Lifecycle

**File:** `crates/bloxide-core/src/event_tag.rs`

```rust
/// Reserved event tag for lifecycle commands.
pub const LIFECYCLE_TAG: u8 = 254;  // Before WILDCARD_TAG (255)

/// Trait for events that may carry lifecycle commands.
pub trait LifecycleEvent: EventTag {
    /// Returns the lifecycle command if this event is one.
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        None
    }
}
```

### 4.2 LifecycleCommand Enum

**File:** `crates/bloxide-supervisor/src/lifecycle.rs`

```rust
/// Lifecycle commands sent to actors via their lifecycle mailbox.
/// Handled at VirtualRoot level, not in user state handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleCommand {
    /// Transition from Init to operational initial state.
    Start,
    /// Transition to Init, report Reset. Actor can be restarted.
    Reset,
    /// Transition to Init, report Stopped. Actor stays in Init.
    Stop,
    /// Health check - respond with Alive.
    Ping,
    // REMOVED: Kill - handled by runtime directly, not as a message
}
```

### 4.3 LifecycleMailbox Type Convention

**Convention:** Supervised actors include a lifecycle mailbox as their first mailbox.

```rust
// Example:
type Mailboxes<R> = (R::Stream<LifecycleCommand>, R::Stream<DomainMsg>);

// Lifecycle mailbox at index 0 for priority (handled by VirtualRoot first).
```

---

## Phase 5: Runtime Kill Capability

### 5.1 KillCap Trait

**File:** `crates/bloxide-core/src/capability.rs`

```rust
/// Capability to kill actors by ID.
/// 
/// Implementations are runtime-specific. The supervisor holds a reference
/// to this capability and calls it when policy says "kill this actor".
pub trait KillCap {
    /// Kill the actor immediately. Runtime aborts the task and drops channels.
    /// No callbacks fire on the actor - it's just gone.
    fn kill(&self, actor_id: ActorId);
}
```

### 5.2 Tokio Implementation

**File:** `runtimes/bloxide-tokio/src/lib.rs`

```rust
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;
use bloxide_core::capability::KillCap;
use bloxide_core::messaging::ActorId;

pub struct TokioKillCap {
    tasks: Arc<Mutex<HashMap<ActorId, JoinHandle<()>>>>,
}

impl KillCap for TokioKillCap {
    fn kill(&self, actor_id: ActorId) {
        if let Some((_, handle)) = self.tasks.lock().unwrap().remove_entry(&actor_id) {
            handle.abort();
        }
    }
}

impl TokioRuntime {
    pub fn kill_cap(&self) -> Arc<dyn KillCap> {
        // Arc to the KillCap held by runtime
    }
}
```

### 5.3 Embassy Implementation

**File:** `runtimes/bloxide-embassy/src/lib.rs`

```rust
impl KillCap for EmbassyKillCap {
    fn kill(&self, _actor_id: ActorId) {
        // Static actors can't be killed - no-op or log warning
        // OR: could drop channels to signal "dead" state
    }
}
```

---

## Phase 6: Runtime Actor Loop Refactor

### 6.1 Remove SupervisedRunLoop Trait

**File:** `crates/bloxide-supervisor/src/service.rs`

DELETE this file and the trait. Runtime actor loops now just:
1. Poll mailboxes (including lifecycle)
2. Call `dispatch()` on any event
3. Report `DispatchOutcome` to supervisor
4. Exit on certain outcomes or runtime-directed kill

### 6.2 Standard Actor Loop (Tokio Example)

**File:** `runtimes/bloxide-tokio/src/actor.rs` (new file)

```rust
pub async fn run_actor<S: MachineSpec + 'static>(
    machine: StateMachine<S>,
    mailboxes: S::Mailboxes<TokioRuntime>,
    actor_id: ActorId,
    notify: TokioSender<ChildLifecycleEvent>,
) {
    let mut machine = machine;
    let mut mailboxes = mailboxes;
    
    loop {
        // Unified mailbox polling (lifecycle + domain)
        let event = mailboxes.next().await;
        
        let outcome = machine.dispatch(event);
        
        // Report outcome to supervisor
        report_outcome::<S>(&outcome, actor_id, &notify);
        
        // No special break needed - supervisor uses KillCap to end task
    }
}

fn report_outcome<S: MachineSpec>(
    outcome: &DispatchOutcome<S::State>,
    actor_id: ActorId,
    notify: &TokioSender<ChildLifecycleEvent>,
) {
    let event = match outcome {
        DispatchOutcome::Started(_) => ChildLifecycleEvent::Started { child_id: actor_id },
        DispatchOutcome::Done(_) => ChildLifecycleEvent::Done { child_id: actor_id },
        DispatchOutcome::Reset => ChildLifecycleEvent::Reset { child_id: actor_id },
        DispatchOutcome::Failed => ChildLifecycleEvent::Failed { child_id: actor_id },
        DispatchOutcome::Stopped => ChildLifecycleEvent::Stopped { child_id: actor_id },
        DispatchOutcome::Alive => ChildLifecycleEvent::Alive { child_id: actor_id },
        DispatchOutcome::NoRuleMatched | 
        DispatchOutcome::HandledNoTransition | 
        DispatchOutcome::Transition(_) => return, // No notification
    };
    
    let _ = notify.try_send(Envelope(actor_id, event));
}
```

---

## Phase 7: Macro Updates

### 7.1 transitions! Macro - Add `fail`

**File:** `crates/bloxide-macros/src/transitions.rs`

Add support for `fail` outcome (in addition to `stay`, `transition`, `reset`):

```rust
transitions![
    SomeError(_) => fail,  // Maps to Guard::Fail
    SomeReset(_) => reset, // Maps to Guard::Reset
]
```

### 7.2 No Init State Injection in Enum

Do NOT inject Init into the user's enum. The `MachineState` type handles it implicitly.

### 7.3 Update root_transitions! Macro

Keep `root_transitions!` for domain events that need global fallback, but lifecycle is always handled by engine at VirtualRoot.

---

## Phase 8: Supervisor Updates

### 8.1 Supervisor Context Has KillCap

**File:** `crates/bloxide-supervisor/src/ctx.rs`

```rust
#[derive(BloxCtx)]
pub struct SupervisorCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    
    /// Capabilities for killing children based on policy.
    #[ctor]
    pub kill_cap: Arc<dyn KillCap>,
    
    // ... other fields ...
}
```

### 8.2 Supervisor Actions

**File:** `crates/bloxide-supervisor/src/actions.rs`

```rust
/// Kill a child actor immediately.
pub fn kill_child<R>(ctx: &impl HasKillCap, child_id: ActorId) {
    ctx.kill_cap().kill(child_id);
}
```

### 8.3 ChildGroup - Track Child States

**File:** `crates/bloxide-supervisor/src/registry.rs`

Update `ChildGroup` to track when children are in Init (so it knows they can be restarted):

```rust
struct ChildEntry {
    id: ActorId,
    lifecycle_ref: ActorRef<LifecycleCommand, R>,
    policy: ChildPolicy,
    restarts: usize,
    in_init: bool,  // NEW: tracks if child is in implicit Init
}
```

---

## Phase 9: Example Blox Updates

For each blox (`ping`, `pong`, `counter`, `pool`, `worker`):

1. Remove any `is_start()` implementations
2. Ensure `initial_state()` returns correct first operational state
3. Use `LifecycleMailbox` convention:

```rust
// Example: WorkerSpec
impl<R: BloxRuntime> MachineSpec for WorkerSpec<R> {
    type State = WorkerState;
    type Event = WorkerEvent<R>;
    type Ctx = WorkerCtx<R>;
    
    // Lifecycle mailbox first, then domain mailboxes
    type Mailboxes<Rt: BloxRuntime> = (
        Rt::Stream<LifecycleCommand>,  // Lifecycle
        Rt::Stream<WorkerCtrl<R>>,  // Control
        Rt::Stream<WorkerMsg>,  // Domain
    );
    
    fn initial_state() -> WorkerState {
        WorkerState::Waiting
    }
    
    // REMOVED: is_start - no longer needed
    
    fn on_init_entry(ctx: &mut WorkerCtx<R>) {
        // Cleanup when entering Init (Reset/Fail/Stop)
        ctx.peers_mut().clear();
        ctx.set_task_id(0);
    }
    
    fn on_init_exit(ctx: &mut WorkerCtx<R>) {
        // Setup when leaving Init (Start)
        // (if needed)
    }
}
```

---

## Phase 10: Test Updates

### 10.1 Update TestRuntime

**File:** `crates/bloxide-core/src/test_utils.rs`

Update `TestRuntime` to support lifecycle mailboxes.

### 10.2 Update All Tests

Remove tests for:
- `is_start()` method
- `machine.start()` / `machine.reset()` direct calls
- `MachinePhase::Init`

Add tests for:
- `MachineState::Init` vs `MachineState::State`
- `Guard::Fail` outcome
- Lifecycle commands through dispatch
- `on_init_entry` firing on Reset/Fail/Stop but not construction

---

## Phase 11: Documentation Updates

### 11.1 Update Existing Specs

| File | Updates |
|------|---------|
| `02-hsm-engine.md` | Rewrite Init/VirtualRoot behavior |
| `08-supervision.md` | Rewrite for observer model, KillCap |
| `12-action-crate-pattern.md` | Add LifecycleMailbox convention |
| `13-factory-injection-and-supervision.md` | Remove two-stream section |
| `AGENTS.md` | Update all invariants |
| `QUICK_REFERENCE.md` | Update decision trees |

---

## File Changes Summary

### New Files
- `spec/architecture/14-unified-lifecycle.md` (this file)
- `runtimes/bloxide-tokio/src/actor.rs` (standard actor loop)
- `runtimes/bloxide-embassy/src/actor.rs` (standard actor loop)

### Modified Files
- `crates/bloxide-core/src/engine.rs` (major refactor)
- `crates/bloxide-core/src/spec.rs` (remove is_start, update trait)
- `crates/bloxide-core/src/transition.rs` (add Guard::Fail)
- `crates/bloxide-core/src/capability.rs` (add KillCap)
- `crates/bloxide-supervisor/src/lifecycle.rs` (remove Kill command)
- `crates/bloxide-supervisor/src/service.rs` (DELETE - remove SupervisedRunLoop)
- `crates/bloxide-supervisor/src/ctx.rs` (add KillCap)
- `runtimes/bloxide-tokio/src/lib.rs` (add KillCap impl)
- `runtimes/bloxide-tokio/src/supervision.rs` (major refactor)
- `runtimes/bloxide-embassy/src/supervision.rs` (major refactor)
- `crates/bloxide-macros/src/transitions.rs` (add fail support)
- All blox spec.rs files (remove is_start, update Mailboxes)
- All example files (update wiring)
- All test files
- All spec docs

---

## Order of Implementation

1. **Phase 1-2:** Core engine types and dispatch refactor
2. **Phase 3-4:** MachineSpec updates, lifecycle event integration
3. **Phase 5-6:** KillCap, runtime actor loop refactor
4. **Phase 7:** Macro updates
5. **Phase 8:** Supervisor updates
6. **Phase 9:** Example blox updates
7. **Phase 10:** Test updates
8. **Phase 11:** Documentation

---

## Key Architectural Insights

### The Unified Mental Model

**Before (dual-track):**
```
LifecycleCommand stream → Runtime intercepts → machine.start/reset (bypasses dispatch)
Domain event streams → dispatch → handler tables
```

**After (unified):**
```
All mailboxes (including lifecycle) → dispatch → VirtualRoot handles lifecycle, state handlers handle domain
```

### Init as Implicit Leaf

```
VirtualRoot (implicit, not entered/exited - just for LCA)
    │
    ├── Init (implicit leaf, auto-generated)
    │       on_entry: S::on_init_entry (fires on Reset/Fail/Stop)
    │       on_exit:  S::on_init_exit (fires on Start)
    │       transitions: [* => stay]  (catch-all for domain events)
    │
    ├── Waiting (user-declared leaf, initial_state())
    ├── Running (user-declared leaf)
    └── Done (user-declared leaf)
```

### Kill vs Stop vs Reset

| Action | Mechanism | Actor Still Exists? | Task Running? | Supervisor Sees |
|--------|-----------|---------------------|---------------|-----------------|
| Reset | LifecycleCommand → dispatch → Init | Yes | Yes | `Reset` |
| Stop | LifecycleCommand → dispatch → Init | Yes | Yes | `Stopped` |
| Kill | Runtime.kill() (KillCap) | No | No | (nothing - actor is gone) |

The supervisor uses Stop/Reset for normal lifecycle management, and Kill when policy decides to permanently remove an actor.
