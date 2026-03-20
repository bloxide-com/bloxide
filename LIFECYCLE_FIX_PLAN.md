# Lifecycle Dispatch Fix Plan

## Issue Summary

The runtime supervision implementations bypass the engine's lifecycle handling, 
returning synthetic outcomes without triggering state transitions or callbacks.

**Severity: P0 - Critical**

## Root Cause Analysis

### Current State
1. The engine's `StateMachine::handle_lifecycle()` method is correctly implemented:
   - `Start` transitions from Init to `initial_state()` (calls `transition_to_state()`)
   - `Reset`/`Stop` call `transition_to_init()` which fires exit chain
   - Returns correct `DispatchOutcome` variants

2. But runtime supervision code doesn't use it:
   - Tokio: `handle_lifecycle_direct()` ignores the machine, returns synthetic outcomes
   - Embassy: `handle_lifecycle_via_dispatch()` same problem
   - Both functions have `_machine: &mut StateMachine<S>` param that's unused

### Why This Happened
1. Lifecycle commands arrive on a separate stream (two-stream architecture)
2. `StateMachine::handle_lifecycle()` was private
3. No public API to invoke lifecycle handling directly
4. Runtimes couldn't dispatch lifecycle events, so they returned synthetic outcomes

## Solution: Make `handle_lifecycle()` Public

Expose the engine's lifecycle handling as a public method that runtimes can call 
directly. This is cleaner than adding `from_lifecycle_command()` to `LifecycleEvent` 
because:
- Doesn't require bloxes to implement constructors
- Engine already has correct implementation  
- Minimal API surface change

## Files to Modify

### 1. `crates/bloxide-core/src/engine.rs`

**Location:** Line 257

**Current:**
```rust
fn handle_lifecycle(&mut self, cmd: LifecycleCommand) -> DispatchOutcome<S::State> {
```

**Change to:**
```rust
pub fn handle_lifecycle(&mut self, cmd: LifecycleCommand) -> DispatchOutcome<S::State> {
```

**Also add documentation:**
```rust
/// Handle lifecycle commands directly.
///
/// This method is used by runtime supervision implementations to process
/// lifecycle commands from the dedicated lifecycle stream, triggering
/// state transitions and callbacks.
///
/// - `Start`: Transitions from Init to `initial_state()`, firing `on_entry`
/// - `Reset`: Transitions to Init, firing exit chain and `on_init_entry`
/// - `Stop`: Transitions to Init, firing exit chain, then signals termination
/// - `Ping`: Returns `Alive` for health checks
pub fn handle_lifecycle(&mut self, cmd: LifecycleCommand) -> DispatchOutcome<S::State> {
```

### 2. `runtimes/bloxide-tokio/src/supervision.rs`

**Location:** Lines 83-94

**Current (BROKEN):**
```rust
/// Handle lifecycle command directly.
fn handle_lifecycle_direct<S: MachineSpec>(
    _machine: &mut StateMachine<S>,
    cmd: LifecycleCommand,
) -> DispatchOutcome<S::State> {
    match cmd {
        LifecycleCommand::Start => DispatchOutcome::Started(MachineState::Init),
        LifecycleCommand::Reset => DispatchOutcome::Reset,
        LifecycleCommand::Stop => DispatchOutcome::Stopped,
        LifecycleCommand::Ping => DispatchOutcome::Alive,
    }
}
```

**Change to:**
```rust
/// Handle lifecycle command by delegating to engine's lifecycle handler.
///
/// This ensures state transitions fire their `on_entry`/`on_exit` callbacks.
pub fn handle_lifecycle_direct<S: MachineSpec>(
    machine: &mut StateMachine<S>,
    cmd: LifecycleCommand,
) -> DispatchOutcome<S::State> {
    machine.handle_lifecycle(cmd)
}
```

### 3. `runtimes/bloxide-embassy/src/supervision.rs`

**Location:** Lines 88-103

**Current (BROKEN):**
```rust
fn handle_lifecycle_via_dispatch<S: MachineSpec>(
    machine: &mut StateMachine<S>,
    cmd: LifecycleCommand,
) -> DispatchOutcome<S::State> {
    // For events that implement LifecycleEvent::from_lifecycle_command,
    // create a wrapped event and dispatch it.
    // For domain-only events, we need to handle specially.
    //
    // Since we can't generically construct events without the trait method,
    // we use a different approach: the machine's engine handles lifecycle
    // commands internally when event.as_lifecycle_command() returns Some.
    //
    // For the two-stream model, we handle lifecycle directly:
    match cmd {
        LifecycleCommand::Start => {
            // This requires the machine to transition from Init
            // For now, return a synthetic outcome
            DispatchOutcome::Started(MachineState::Init)
        }
        LifecycleCommand::Reset => DispatchOutcome::Reset,
        LifecycleCommand::Stop => DispatchOutcome::Stopped,
        LifecycleCommand::Ping => DispatchOutcome::Alive,
    }
}
```

**Change to:**
```rust
/// Handle lifecycle command by delegating to engine's lifecycle handler.
///
/// This ensures state transitions fire their `on_entry`/`on_exit` callbacks.
pub fn handle_lifecycle_via_dispatch<S: MachineSpec>(
    machine: &mut StateMachine<S>,
    cmd: LifecycleCommand,
) -> DispatchOutcome<S::State> {
    machine.handle_lifecycle(cmd)
}
```

## Test Cases to Add

### Test File: `crates/bloxide-core/src/tests/lifecycle_dispatch.rs` (NEW)

```rust
//! Tests for lifecycle dispatch through the engine.
//!
//! These tests verify that lifecycle commands trigger proper state transitions
//! and callbacks, not just synthetic outcomes.

#[cfg(all(test, feature = "std"))]
mod tests {
    use crate::lifecycle::LifecycleCommand;
    use crate::engine::{DispatchOutcome, MachineState, StateMachine};
    use crate::spec::MachineSpec;
    use crate::test_utils::TestRuntime;
    use crate::event_tag::{EventTag, LifecycleEvent};
    use crate::topology::StateTopology;
    use crate::transition::ActionResult;
    use crate::messaging::Envelope;
    use core::marker::PhantomData;

    // ── Test Spy State Machine ─────────────────────────────────────────────

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestState {
        Init,  // Will be tracked by MachineState::Init
        Running,
        Done,
    }

    impl StateTopology for TestState {
        const STATE_COUNT: usize = 3;
        fn parent(self) -> Option<Self> { None }
        fn is_leaf(self) -> bool { true }
        fn path(self) -> &'static [Self] { &[self] }
        fn as_index(self) -> usize { self as usize }
    }

    #[derive(Debug)]
    enum TestEvent {
        Lifecycle(LifecycleCommand),
        Msg(u32),
    }

    impl EventTag for TestEvent {
        fn event_tag(&self) -> u8 {
            match self {
                TestEvent::Lifecycle(_) => 254, // LIFECYCLE_TAG
                TestEvent::Msg(_) => 0,
            }
        }
    }

    impl LifecycleEvent for TestEvent {
        fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
            match self {
                TestEvent::Lifecycle(cmd) => Some(*cmd),
                _ => None,
            }
        }
    }

    impl From<Envelope<u32>> for TestEvent {
        fn from(env: Envelope<u32>) -> Self { TestEvent::Msg(env.1) }
    }

    /// Spy context that tracks which callbacks fire.
    #[derive(Default)]
    struct SpyCtx {
        on_entry_calls: Vec<TestState>,
        on_exit_calls: Vec<TestState>,
        on_init_entry_calls: u32,
    }

    struct TestSpec<R>(PhantomData<R>);

    impl<R: crate::capability::BloxRuntime> MachineSpec for TestSpec<R> {
        type State = TestState;
        type Event = TestEvent;
        type Ctx = SpyCtx;
        type Mailboxes<Rt: crate::capability::BloxRuntime> = (Rt::Stream<u32>,);

        const HANDLER_TABLE: &'static [&'static crate::spec::StateFns<Self>] = &[
            // Init - empty, handled by engine
            crate::spec::StateFns { on_entry: &[], on_exit: &[], transitions: &[] },
            // Running
            crate::spec::StateFns {
                on_entry: &[|ctx: &mut SpyCtx| {
                    ctx.on_entry_calls.push(TestState::Running);
                    ActionResult::Ok
                }],
                on_exit: &[|ctx: &mut SpyCtx| {
                    ctx.on_exit_calls.push(TestState::Running);
                    ActionResult::Ok
                }],
                transitions: &[],
            },
            // Done
            crate::spec::StateFns {
                on_entry: &[|ctx: &mut SpyCtx| {
                    ctx.on_entry_calls.push(TestState::Done);
                    ActionResult::Ok
                }],
                on_exit: &[],
                transitions: &[],
            },
        ];

        fn initial_state() -> TestState { TestState::Running }
        fn is_terminal(state: &TestState) -> bool { matches!(state, TestState::Done) }
        fn is_error(_state: &TestState) -> bool { false }
        
        fn on_init_entry(ctx: &mut SpyCtx) {
            ctx.on_init_entry_calls += 1;
        }
    }

    // ── Test Cases ─────────────────────────────────────────────────────────

    #[test]
    fn start_from_init_fires_on_entry() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        
        // Machine starts in Init
        assert_eq!(machine.current_state(), MachineState::Init);
        
        // Dispatch Start via handle_lifecycle
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
        
        // Verify outcome
        assert_eq!(outcome, DispatchOutcome::Started(MachineState::State(TestState::Running)));
        
        // Verify state changed
        assert_eq!(machine.current_state(), MachineState::State(TestState::Running));
        
        // CRITICAL: Verify on_entry fired
        assert_eq!(machine.ctx().on_entry_calls, vec![TestState::Running]);
        
        // Verify NO on_exit (no transition from operational state)
        assert!(machine.ctx().on_exit_calls.is_empty());
    }

    #[test]
    fn start_from_operational_is_noop() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        
        // Start first
        machine.handle_lifecycle(LifecycleCommand::Start);
        machine.ctx_mut().on_entry_calls.clear(); // Reset spy
        
        // Dispatch Start again
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
        
        // Verify outcome shows no transition
        assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
        
        // Verify state unchanged
        assert_eq!(machine.current_state(), MachineState::State(TestState::Running));
        
        // CRITICAL: Verify NO callbacks fired
        assert!(machine.ctx().on_entry_calls.is_empty());
        assert!(machine.ctx().on_exit_calls.is_empty());
    }

    #[test]
    fn reset_fires_exit_chain_and_on_init_entry() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        
        // Start and verify we're in Running
        machine.handle_lifecycle(LifecycleCommand::Start);
        assert_eq!(machine.current_state(), MachineState::State(TestState::Running));
        
        machine.ctx_mut().on_entry_calls.clear();
        machine.ctx_mut().on_exit_calls.clear();
        
        // Dispatch Reset
        let outcome = machine.handle_lifecycle(LifecycleCommand::Reset);
        
        // Verify outcome
        assert_eq!(outcome, DispatchOutcome::Reset);
        
        // Verify state changed back to Init
        assert_eq!(machine.current_state(), MachineState::Init);
        
        // CRITICAL: Verify on_exit fired for Running
        assert_eq!(machine.ctx().on_exit_calls, vec![TestState::Running]);
        
        // CRITICAL: Verify on_init_entry fired
        assert_eq!(machine.ctx().on_init_entry_calls, 1);
    }

    #[test]
    fn stop_fires_exit_chain_and_reports_stopped() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        
        // Start
        machine.handle_lifecycle(LifecycleCommand::Start);
        machine.ctx_mut().on_entry_calls.clear();
        machine.ctx_mut().on_exit_calls.clear();
        
        // Dispatch Stop
        let outcome = machine.handle_lifecycle(LifecycleCommand::Stop);
        
        // Verify outcome
        assert_eq!(outcome, DispatchOutcome::Stopped);
        
        // Verify state changed to Init
        assert_eq!(machine.current_state(), MachineState::Init);
        
        // CRITICAL: Verify on_exit fired
        assert_eq!(machine.ctx().on_exit_calls, vec![TestState::Running]);
        
        // CRITICAL: Verify on_init_entry fired
        assert_eq!(machine.ctx().on_init_entry_calls, 1);
    }

    #[test]
    fn ping_returns_alive_without_state_change() {
        let ctx = SpyCtx::default();
        let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(ctx);
        
        // Start
        machine.handle_lifecycle(LifecycleCommand::Start);
        machine.ctx_mut().on_entry_calls.clear();
        
        // Dispatch Ping
        let outcome = machine.handle_lifecycle(LifecycleCommand::Ping);
        
        // Verify outcome
        assert_eq!(outcome, DispatchOutcome::Alive);
        
        // Verify state unchanged
        assert_eq!(machine.current_state(), MachineState::State(TestState::Running));
        
        // Verify NO callbacks fired
        assert!(machine.ctx().on_entry_calls.is_empty());
        assert!(machine.ctx().on_exit_calls.is_empty());
    }
}
```

### Integration Test for Tokio Runtime

Add to `runtimes/bloxide-tokio/src/supervision.rs` tests:

```rust
#[test]
fn handle_lifecycle_direct_uses_engine_dispatch() {
    // This test verifies the fix: handle_lifecycle_direct must delegate
    // to machine.handle_lifecycle(), not return synthetic outcomes.
    
    use bloxide_core::engine::StateMachine;
    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::engine::DispatchOutcome;
    use bloxide_core::engine::MachineState;
    
    // Create a simple test machine
    // ... (use TestSpec from above)
    
    let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(SpyCtx::default());
    
    // Call handle_lifecycle_direct (the function being fixed)
    let outcome = handle_lifecycle_direct(&mut machine, LifecycleCommand::Start);
    
    // CRITICAL ASSERTIONS:
    
    // 1. Outcome must indicate Started with actual state, not MachineState::Init
    assert!(matches!(outcome, DispatchOutcome::Started(MachineState::State(_))));
    
    // 2. Machine must have transitioned out of Init
    assert_ne!(machine.current_state(), MachineState::Init);
    
    // 3. on_entry MUST have fired (this was the bug!)
    assert!(!machine.ctx().on_entry_calls.is_empty(), 
        "handle_lifecycle_direct MUST trigger on_entry callbacks");
}
```

### Integration Test for Embassy Runtime

Add to `runtimes/bloxide-embassy/src/supervision.rs` tests:

```rust
#[test]
fn handle_lifecycle_via_dispatch_uses_engine_dispatch() {
    // Same test as Tokio version
    // Verifies Embassy implementation delegates to engine
    
    let mut machine = StateMachine::<TestSpec<TestRuntime>>::new(SpyCtx::default());
    let outcome = handle_lifecycle_via_dispatch(&mut machine, LifecycleCommand::Start);
    
    assert!(matches!(outcome, DispatchOutcome::Started(MachineState::State(_))));
    assert_ne!(machine.current_state(), MachineState::Init);
    assert!(!machine.ctx().on_entry_calls.is_empty(),
        "handle_lifecycle_via_dispatch MUST trigger on_entry callbacks");
}
```

## Acceptance Criteria

1. **No synthetic outcomes**: `handle_lifecycle_direct` and `handle_lifecycle_via_dispatch` 
   call `machine.handle_lifecycle(cmd)` instead of returning synthetic outcomes

2. **Callback firing**: Lifecycle commands trigger `on_entry`/`on_exit` callbacks

3. **Correct state reporting**: `Started` outcome contains `MachineState::State(initial)`, 
   not `MachineState::Init`

4. **report_outcome works**: After fix, `report_outcome` receives proper state variants
   and sends correct `ChildLifecycleEvent` types

5. **All existing tests pass**: No regressions in bloxide-core, bloxide-supervisor,
   ping, pong, pool, worker blox tests

## Implementation Order

1. Make `handle_lifecycle()` public in `engine.rs` (add docs)
2. Replace `handle_lifecycle_direct()` in Tokio supervision
3. Replace `handle_lifecycle_via_dispatch()` in Embassy supervision
4. Add test file `crates/bloxide-core/src/tests/lifecycle_dispatch.rs`
5. Add integration tests to Tokio and Embassy supervision
6. Add test module reference to `crates/bloxide-core/src/tests/mod.rs`:
   ```rust
   #[cfg(all(test, feature = "std"))]
   mod lifecycle_dispatch;
   ```
7. Run `cargo test --workspace` and verify all pass

## Files Changed Summary

| File | Change |
|------|--------|
| `crates/bloxide-core/src/engine.rs` | Make `handle_lifecycle()` public, add docs |
| `runtimes/bloxide-tokio/src/supervision.rs` | Fix `handle_lifecycle_direct()` to use engine |
| `runtimes/bloxide-embassy/src/supervision.rs` | Fix `handle_lifecycle_via_dispatch()` to use engine |
| `crates/bloxide-core/src/tests/lifecycle_dispatch.rs` | NEW: comprehensive lifecycle tests |
| `crates/bloxide-core/src/tests/mod.rs` | Reference new test module |

## Risk Assessment

**Low Risk**: The fix is straightforward - delegate to existing correct implementation.
The engine's `handle_lifecycle()` is already well-tested. We're just exposing it.

**Edge Cases**: 
- Double Start while in Init: Already handled (returns `HandledNoTransition`)
- Reset while in Init: Already handled (no-op exit chain, just `on_init_entry`)

## Notes

- The `Guard::Fail` variant mentioned in the review IS already implemented in `transition.rs`
- The `is_start()` removal from `MachineSpec` is intentional and part of the unified lifecycle design
- The `LifecycleEvent` trait doesn't need `from_lifecycle_command()` for this fix
