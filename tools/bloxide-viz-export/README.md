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

Output goes to `./bloxide-viz-output/` by default (or `[output-dir]` if provided). One `.json` file is written per discovered blox crate.

## How it works

1. **Scans** the workspace for `blox.toml` files.
2. **Parses** each `blox.toml` to extract:
   - State topology (states, composite/parent attributes, initial/terminal/error flags)
   - `[[topology.transitions]]` entries (events, targets, actions, guards)
   - Event/message definitions and context fields
3. **Writes** a JSON file per blox.

## JSON → Visualizer

Open the [bloxide-visualizer](../bloxide-visualizer/) in a browser, click **Import .json**, and select the generated `.json` file.
