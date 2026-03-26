# Blox Visualization Improvement Notes

**Date**: 2025-03-25  
**Context**: Review of `blox-viz` PNG generator for static blox diagrams

---

## Current State

The `blox-viz` crate at `crates/blox-viz/` generates PNG diagrams from `BloxDiagramSnapshot` structures. Example command:

```bash
cd crates/blox-viz
cargo run --bin blox-diagram --features "cli png" -- --example ping --png /tmp/ping.png
```

**Key Files:**
- `src/layout.rs` - Tree layout algorithm, `snapshot_to_flow()`
- `src/svg.rs` - SVG rendering with styled nodes/edges
- `src/raster.rs` - PNG generation via resvg
- `src/snapshot.rs` - Data structures (`BloxDiagramSnapshot`, `StateSnapshot`, etc.)

---

## Problem Statement

**Current visualization does not look sufficiently "tree-like"**

The hierarchy is present in data (parent-child via `StateSnapshot.parent_id`) but not visually prominent.

---

## Issues Identified

1. **Weak visual hierarchy** - Composite states (e.g., "Operating") contain children ("Active", "Paused") but containment is subtle
2. **Missing tree connectors** - Hierarchy edges are explicitly removed in `svg.rs`:
   ```rust
   edge_renders.retain(|er| !er.class_name.contains("bv-edge-hierarchy"));
   ```
3. **Flat appearance** - States appear scattered rather than in clear tree levels
4. **Terminal states not visually distinct** - Done/Error don't clearly read as "endpoints"

---

## Suggested Improvements

### 1. Add Tree-Style Connectors
Instead of removing hierarchy edges, render them as tree-style elbow connectors:
- T-shape or L-shape joints from parent to children
- Dashed or lighter weight than transition edges
- Located in `svg.rs` around line 48 where `edge_renders.retain()` is called

### 2. Stronger Containment Visuals
For `NodeKind::Composite` in `svg.rs`:
- Subtle background fill to show "container" nature
- Distinct border styling (already has dashed border)
- Visual grouping that clearly encompasses children

### 3. Tree-Structured Layout
In `layout.rs`, implement a "strict tree" layout mode:
- Root states at top level
- Children in rows below parents
- Parents centered above their children
- Consistent vertical spacing per level

**Constants to adjust:**
```rust
const LEVEL_HEIGHT: f32 = 180.0;  // Vertical space between tree levels
const SIBLING_GAP: f32 = 60.0;     // Horizontal space between siblings
const PARENT_CHILD_INDENT: f32 = 40.0;
```

### 4. Visual Indicators for State Types
- **Composite states**: Folder-like appearance with distinct header (partially implemented)
- **Leaf states**: Simple boxes (current)
- **Terminal states** (Done/Error): Rounded/pill-shaped or distinct color to indicate endpoints
- **Init state**: Already distinct (green, rounded)

---

## Open Design Question: Terminal States

### Semantic Discussion

**Current model:**
- **Init**: Engine-implicit "waiting" state (actor constructed, not started)
- **Done**: Terminal leaf state (`is_terminal()` returns true) - actor completed work
- **Reset**: Transitions from any state back to `initial_state()` (Active for Ping), NOT to Init
- **Start**: Init → initial_state() transition (runtime-called)

**Key point**: Done ≠ Init. They have different semantics:
- Done means completion; actor can be Reset to restart
- Init is a pre-operational state

### Visualization Options

**Option A: Distinct Terminal (Current Design)**
- Done is a separate leaf state at the bottom
- No visual connection back to Init
- Pros: Semantically accurate per bloxide framework
- Cons: May not show "full lifecycle" clearly

**Option B: Terminal as Return to Init**
- Show Done with transition edge back to Init
- Label: "Reset to restart" or similar
- Pros: Emphasizes cyclical lifecycle
- Cons: Not strictly accurate (Reset goes to initial_state, not Init)

**Option C: Hybrid Approach**
- Keep Done as distinct terminal state
- Add dashed lifecycle edge showing Reset returns to Active (initial_state)
- Add Init as distinct entry point
- Shows: Entry → Operation → Terminal → (via Reset) → Operation

**Reference**: See `spec/architecture/08-supervision.md` for lifecycle command semantics:
> "Lifecycle commands flow through dispatch() at VirtualRoot level... Reset goes to user-defined initial_state()"

### Recommendation

For MVP: **Option A** (current) with possible enhancement to show Reset capability.

For user clarity: Consider **Option C** with lifecycle edges as secondary/dashed lines.

---

## Implementation Priority

1. **High Impact, Low Effort**: Add tree-style hierarchy connectors (elbow joints) in `svg.rs`
2. **Visual Polish**: Enhance composite state containment styling
3. **Layout Refinement**: Implement strict tree layout in `layout.rs`
4. **Terminal State Design**: Decide between Options A/B/C above

---

## Related Documentation

- `spec/architecture/02-hsm-engine.md` - StateTopology, MachineSpec
- `spec/architecture/08-supervision.md` - Lifecycle events (Started, Done, Failed, Reset, Stopped)
- `crates/blox-viz/src/snapshot.rs` - Example ping blox snapshot for reference

---

## Next Steps

[ ] Implement tree connectors
[ ] Review terminal state visualization approach
[ ] Consider auto-generating snapshots from `StateTopology` derive macro metadata (future)
