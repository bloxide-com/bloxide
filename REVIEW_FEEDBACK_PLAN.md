# Review Feedback Resolution Plan

## Overview

Following the implementation of LIFECYCLE_FIX_PLAN.md, a code review identified 7 issues
(3 P2, 4 P3) that should be addressed. This plan provides implementation guidance for
each issue.

## Priority 2 Issues (Should Fix Before Merge)

### 1. KillCap trait bounds mismatch with architecture spec

**Location**: `crates/bloxide-core/src/capability.rs:118-126`

**Problem**: The `KillCap` trait does not declare `Clone + Send + Sync + 'static` bounds,
but the architecture spec in `spec/architecture/08-supervision.md` shows these bounds as
required. This could cause issues when supervisors try to hold `Rc<dyn KillCap>` references
or when threading is involved.

**Solution**: Update the `KillCap` trait definition:

```rust
// Current (probably):
pub trait KillCap { ... }

// Should be:
pub trait KillCap: Clone + Send + Sync + 'static { ... }
```

**Verification**: Check if this breaks any existing implementations in `bloxide-tokio`.

---

### 2. Duplicate LifecycleCommand types across crates

**Location**: `runtimes/bloxide-tokio/src/supervision.rs:13-20`

**Problem**: Two `LifecycleCommand` types exist:
- `bloxide_core::lifecycle::LifecycleCommand` (new)
- `bloxide_supervisor::lifecycle::LifecycleCommand` (old)

The Tokio supervision code imports both as `LifecycleCommand` and `SupLifecycleCommand`
and does manual conversion between them. This is fragile and creates maintenance burden.

**Solution Options**:
1. **Preferred**: Make `bloxide-supervisor` re-export from `bloxide-core`:
   ```rust
   // In bloxide-supervisor/src/lifecycle.rs:
   pub use bloxide_core::lifecycle::LifecycleCommand;
   ```
2. **Alternative**: Remove conversion logic and use only `bloxide_core::LifecycleCommand`
   throughout runtimes.

**Files to update**:
- `crates/bloxide-supervisor/src/lifecycle.rs` - re-export or remove
- `runtimes/bloxide-tokio/src/supervision.rs` - use single type
- `runtimes/bloxide-embassy/src/supervision.rs` - use single type

---

### 3. Terminology inconsistency: Reset vs Terminate

**Location**: `QUICK_REFERENCE.md:196-210`

**Problem**: The code uses `LifecycleCommand::Reset` in bloxide-core but some documentation
still references "Terminate". This naming inconsistency will confuse users.

**Solution**: Update QUICK_REFERENCE.md to use "Reset" consistently:

```markdown
// Before:
Terminate: Actor should stop permanently...

// After:
Reset: Actor returns to Init state (restartable)...
Stop: Actor should stop permanently...
```

**Additional locations to check**:
- `spec/architecture/*.md` - any references to "Terminate"
- `crates/bloxide-supervisor/src/` - comments and docs

---

### 4. CounterEvent cannot receive lifecycle commands

**Location**: `crates/bloxes/counter/src/events.rs:10-16`

**Problem**: `CounterEvent` implements `LifecycleEvent::as_lifecycle_command` to always
return `None` because it lacks a `Lifecycle(LifecycleCommand)` variant. This means counter
actors cannot be supervised via the unified lifecycle model. This is inconsistent with
ping/pong/pool/worker bloxes which DO accept lifecycle commands.

**Solution**: Add the Lifecycle variant to CounterEvent:

```rust
#[derive(Debug)]
pub enum CounterEvent {
    Lifecycle(LifecycleCommand),  // ADD THIS
    Tick,
    SetTarget { count: u32 },
    GetCount,
    CountResult { count: u32 },
}
```

Update `LifecycleEvent` impl:

```rust
impl LifecycleEvent for CounterEvent {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        match self {
            CounterEvent::Lifecycle(cmd) => Some(*cmd),  // ADD THIS
            _ => None,
        }
    }
}
```

**Alternative**: If counter is intentionally a "domain-only" actor, document this decision
in `spec/bloxes/counter.md`.

---

## Priority 3 Issues (Can Fix After Merge)

### 5. Unused import warning in embassy supervision

**Location**: `runtimes/bloxide-embassy/src/supervision.rs:5`

**Problem**: The import `event_tag::LifecycleEvent` is unused and generates a compiler warning.
This is leftover from the refactoring.

**Solution**: Remove the unused import:

```rust
// DELETE this line:
use bloxide_core::event_tag::LifecycleEvent;
```

---

### 6. Deprecated SupervisedRunLoop still widely used

**Location**: `crates/bloxide-supervisor/src/service.rs:5-21`

**Problem**: `SupervisedRunLoop` trait is marked deprecated but is still imported and used
in `bloxide-tokio`, `bloxide-embassy`, and the supervisor prelude. This creates a confusing
transitional state.

**Solution Options**:
1. **Complete migration**: Remove `SupervisedRunLoop` trait and update all usages to
   use unified dispatch through `LifecycleEvent` trait
2. **Remove deprecation**: If migration is not ready, remove the `#[deprecated]` attribute
   until migration is complete

**Recommendation**: Check with team on migration timeline. If migration is planned for
next sprint, keep deprecation. If timeline is uncertain, remove deprecation notice.

---

### 7. MachineState::unwrap_state could mask Init bugs

**Location**: `crates/bloxide-core/src/engine.rs:46-54`

**Problem**: `unwrap_state` returns a `Default` value when called on `MachineState::Init`.
This could silently hide bugs where code expects an operational state but gets a default.

**Solution Options**:
1. **Return Option<S>**: Make the caller explicitly handle the Init case
   ```rust
   pub fn unwrap_state(&self) -> Option<S> {
       match self {
           MachineState::State(s) => Some(*s),
           MachineState::Init => None,
       }
   }
   ```
2. **Rename to make behavior clear**:
   ```rust
   pub fn unwrap_state_or_default(&self) -> S { ... }
   ```
3. **Add documentation**: If behavior is intentional, document why Default is returned
   for Init.

**Recommendation**: Option 1 (return `Option<S>`) is safest but requires updating callers.
Check all usages of `unwrap_state` before deciding.

---

## Implementation Order

1. **P3 quick win**: Remove unused import (#5)
   - Single line change, no risk

2. **P2 correctness fixes**:
   - #4 CounterEvent (add Lifecycle variant)
   - #1 KillCap bounds (add trait bounds)
   - #3 Reset/Terminate naming (documentation update)
   - #2 LifecycleCommand consolidation (requires more care)

3. **P3 design decisions**:
   - #6 SupervisedRunLoop (decide on migration vs remove deprecation)
   - #7 MachineState::unwrap_state (evaluate impact of Option<S> change)

4. **Final verification**: `cargo test --workspace`

---

## Files Changed Summary

| File | Issue | Change |
|------|-------|--------|
| `runtimes/bloxide-embassy/src/supervision.rs` | #5 | Remove unused import |
| `crates/bloxide-core/src/capability.rs` | #1 | Add KillCap trait bounds |
| `crates/bloxes/counter/src/events.rs` | #4 | Add Lifecycle variant |
| `QUICK_REFERENCE.md` | #3 | Fix Reset vs Terminate naming |
| `crates/bloxide-supervisor/src/lifecycle.rs` | #2 | Re-export from core or remove |
| `runtimes/bloxide-tokio/src/supervision.rs` | #2 | Use single LifecycleCommand type |
| `crates/bloxide-supervisor/src/service.rs` | #6 | Decide on deprecation |
| `crates/bloxide-core/src/engine.rs` | #7 | Evaluate unwrap_state return type |

---

## Acceptance Criteria

1. All compiler warnings resolved (`cargo check --workspace` clean)
2. All tests pass (`cargo test --workspace`)
3. Documentation uses consistent terminology (Reset, not Terminate)
4. KillCap trait matches architecture spec bounds
5. Single LifecycleCommand type used across runtimes
6. CounterEvent compatible with supervision (or documented as intentional limitation)
