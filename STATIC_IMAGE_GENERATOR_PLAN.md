# Static image generator plan

This document records the plan for Bloxide’s **static** diagram export: layout and rendering of a single blox (from a snapshot) to **SVG** and optionally **PNG**, without depending on Bloxflow or any interactive UI.

## Goals

- **Stable diagram data** that tools and (eventually) proc macros can emit.
- **Deterministic static output** suitable for docs, CI artifacts, and golden tests.
- **No Bloxflow** in the export path; the commercial editor will use **wgpu** later and can share the same **geometry intermediate representation** (IR).

## Current state

- `crates/blox-viz` already provides `BloxDiagramSnapshot`, layout, Mermaid/DOT, and (with features) `snapshot_to_svg` and `snapshot_to_png`.
- SVG export currently goes through **`bloxflow_core`** (`HsmFlowNode`, `FlowPaintCtx`). That dependency should be **removed** from the static path.
- The experimental web integration **`integrations/blox-viz-web`** was removed; interactive product work belongs in the **commercial app**, not this repository.

## Target architecture

1. **Data model** — Versioned snapshot types (`BloxDiagramSnapshot`, states, transitions, hierarchy). Optional `serde` for JSON fixtures. Explicit **schema/format version** so old files fail clearly.
2. **Validation** — Pure checks: unique ids, valid `parent_id` references, acyclic hierarchy, transition endpoints exist; structured errors for CLI and tests.
3. **Layout** — Algorithms that consume a snapshot and produce a **geometry IR**: placed rectangles for states, routes for edges, labels, implicit Init pseudostate. No dependency on external graph-widget crates.
4. **SVG backend** — Build SVG (XML string or a minimal SVG helper) from the geometry IR.
5. **Raster backend** — Rasterize the SVG string with **resvg** (or equivalent) for PNG export behind an optional feature, matching existing behavior where possible.
6. **CLI** — Keep or consolidate a small binary (e.g. `blox-diagram`) that reads JSON (optional) and writes `.svg` / `.png`.

## Crate layout (recommended)

| Crate | Role |
|-------|------|
| `blox-diagram` | Types + validation + optional serde; no layout or paint. |
| `blox-render-static` | Layout → geometry IR → SVG → optional PNG; depends on `blox-diagram`. |
| `blox-viz` | Either a thin re-export + CLI, or folded into `blox-render-static` once APIs stabilize. |

A single crate with feature flags is acceptable until the split pays off; the **logical** split above should still guide module boundaries.

## Migration steps

1. Introduce **geometry IR** and an SVG emitter that can render at least one **hand-authored** snapshot end-to-end (e.g. ping example).
2. **Port** layout from `blox-viz/src/layout.rs` off `bloxflow_core` types onto the geometry IR; preserve behavior on the reference snapshot (golden tests).
3. Replace Bloxflow-based SVG generation; remove `hsm_flow_node`’s role in **static** export (delete or narrow that module).
4. Add **golden SVG** tests (normalized or hashed) for the reference snapshot; add PNG golden tests if useful.
5. Remove **`bloxflow-core`** from the workspace when no crate needs it for diagrams.
6. Document the **format version** policy and where snapshot JSON lives for tests.

## Out of scope for this plan

- Interactive editing, wgpu, or the commercial application (separate repository).
- Automatic emission of snapshots from `MachineSpec` (future macros or codegen; may consume the same types as this plan).

## Success criteria

- `cargo build` / `cargo test` for the diagram crates succeed without `bloxflow`.
- Static SVG (and optional PNG) output is stable under layout refactors (guarded by golden tests).
- Clear versioning story for on-disk snapshot JSON.
