# bloxide-viz-export

CLI tool that scans a Bloxide workspace and exports visualization specs as JSON.

## Purpose

Reads `blox.toml` files directly and generates machine-readable JSON that the [bloxide-visualizer](../bloxide-visualizer/) can load. Since `blox.toml` is the single source of truth, the exported JSON always reflects the declarative intent of each blox.

## Usage

```bash
cd tools/bloxide-viz-export
cargo run -- <path-to-bloxide-workspace> [output-dir]
```

Example against the main bloxide repo:

```bash
cargo run -- /repos/internal/bloxide
```

Exported JSON contains local source paths and is gitignored; regenerate it from the checkout you intend to visualize.

Output goes to `./bloxide-viz-output/` by default (or `[output-dir]` if provided). One `.json` file is written per discovered blox crate, plus one per `system.toml` application manifest (see below).

## How it works

1. **Scans** the workspace for `blox.toml` files **and `system.toml` files**.
2. **Parses** each `blox.toml` to extract:
   - State topology (states, composite/parent attributes, initial/error flags)
   - `[[topology.transitions]]` entries (events, targets, actions, guards)
   - Event/message definitions and context fields
3. **Parses** each `system.toml` (application wiring manifest, e.g. `examples/tokio-demo/system.toml`) into a system spec whose `wiring` field carries the actor graph:
   - `actors` — actor instances and their blox crates
   - `connections` — channel connections (from/to actor, message type, capacity)
   - `supervisors` — supervision strategy and children
4. **Writes** a JSON file per blox and per system.

System specs have no state topology — only the wiring graph is populated (`wiring: Some(...)`); blox specs have `wiring: None`. The visualizer uses the wiring graph to render the System View (actor connection diagram).

## JSON → Visualizer

Open the [bloxide-visualizer](../bloxide-visualizer/) in a browser, click **Import .json**, and select the generated `.json` file.
