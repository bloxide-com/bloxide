# Unified Lifecycle Implementation Progress

## Status: Implementation Complete, Critical Bug Found

### What's Been Accomplished

#### Phase 1-11: Core Implementation (COMPLETED)
- ✅ `MachineState<S>` enum created (replaces `MachinePhase`)
- ✅ `Guard::Fail` variant added to transition.rs
- ✅ `DispatchOutcome` updated with new variants
- ✅ `LifecycleCommand` and `ChildLifecycleEvent` in bloxide-core
- ✅ `LifecycleEvent` trait added
- ✅ Engine's `handle_lifecycle()` correctly implemented
- ✅ All blox events updated with `LifecycleEvent` impl
- ✅ All workspace tests compile and pass (68 tests)

### Critical Bug Found During Review

**Severity: P0 - Critical**

The runtime supervision implementations bypass the engine's lifecycle handling:
- `runtimes/bloxide-tokio/src/supervision.rs::handle_lifecycle_direct()` returns synthetic outcomes
- `runtimes/bloxide-embassy/src/supervision.rs::handle_lifecycle_via_dispatch()` does the same
- Both functions have `_machine` parameter that is **never used**
- **Result**: No callbacks fire during lifecycle transitions

**Impact**:
- Supervised actors remain stuck in Init
- `on_entry`/`on_exit` never fire for Start/Reset/Stop
- Restart logic fails
- State cleanup never occurs

### Fix Plan

See **`LIFECYCLE_FIX_PLAN.md`** for the complete fix plan with:
- Root cause analysis
- Exact code changes for 3 files
- Comprehensive test cases (7 new tests)
- Acceptance criteria
- Implementation order

### Quick Summary of Fix

1. Make `StateMachine::handle_lifecycle()` public (it's already correctly implemented)
2. Replace synthetic outcome functions with simple delegation:
   ```rust
   pub fn handle_lifecycle_direct<S: MachineSpec>(
       machine: &mut StateMachine<S>,
       cmd: LifecycleCommand,
   ) -> DispatchOutcome<S::State> {
       machine.handle_lifecycle(cmd)
   }
   ```

### Test Cases to Add

| Test | Purpose |
|------|---------|
| `start_from_init_fires_on_entry` | Verify Start triggers on_entry callback |
| `start_from_operational_is_noop` | Verify double-Start is idempotent |
| `reset_fires_exit_chain_and_on_init_entry` | Verify Reset fires exit chain |
| `stop_fires_exit_chain_and_reports_stopped` | Verify Stop fires exit chain |
| `ping_returns_alive_without_state_change` | Verify Ping doesn't change state |
| `handle_lifecycle_direct_uses_engine_dispatch` | Integration test for Tokio |
| `handle_lifecycle_via_dispatch_uses_engine_dispatch` | Integration test for Embassy |

### Files to Modify

| File | Change |
|------|--------|
| `crates/bloxide-core/src/engine.rs` | Make `handle_lifecycle()` public |
| `runtimes/bloxide-tokio/src/supervision.rs` | Fix `handle_lifecycle_direct()` |
| `runtimes/bloxide-embassy/src/supervision.rs` | Fix `handle_lifecycle_via_dispatch()` |
| `crates/bloxide-core/src/tests/lifecycle_dispatch.rs` | NEW: comprehensive tests |
| `crates/bloxide-core/src/tests/mod.rs` | Reference new test module |

### Next Steps

1. **Read** `LIFECYCLE_FIX_PLAN.md` for detailed instructions
2. **Apply** the code changes
3. **Add** the test cases
4. **Run** `cargo test --workspace` to verify
5. **Commit** the fix

### Reference

- Fix Plan: `LIFECYCLE_FIX_PLAN.md`
- Spec: `spec/architecture/14-unified-lifecycle.md`
- Engine: `crates/bloxide-core/src/engine.rs`
