# bloxide-viz-export

CLI tool that scans a Bloxide workspace and exports visualization specs as JSON.

## Purpose

Instead of maintaining hand-written `.md` spec files that can drift from the actual Rust source code, this tool reads the **real** blox crate source (states, transitions, actions, messages, context) and generates a machine-readable JSON file that the [bloxide-visualizer](../bloxide-visualizer/) can load.

## Usage

```bash
cd tools/bloxide-viz-export
cargo run -- <path-to-bloxide-workspace> [output-dir]
```

Example against the main bloxide repo:

```bash
cargo run -- /home/bboganware/repos/bloxide
```

Output goes to `./bloxide-viz-output/` by default (or `[output-dir]` if provided). One `.json` file is written per discovered blox crate.

## How it works

1. **Scans** the workspace for crates containing a `blox.toml` with `[event]` and `[topology]` sections, along with a `src/spec.rs` implementing `MachineSpec for`.
2. **Parses** `src/spec.rs` with `syn` to extract:
   - State topology (variants, composite/parent attributes)
   - `StateFns` constants (on_entry, on_exit, transitions)
   - `[[topology.transitions]]` entries (parsed from `blox.toml`)
3. **Reads** `src/events.rs` for event/message definitions and `src/ctx.rs` for context fields.
4. **Writes** a JSON file per blox.

## JSON → Visualizer

Open the [bloxide-visualizer](../bloxide-visualizer/) in a browser, click **Import .md / .json**, and select the generated `.json` file. The visualizer will render the heatmap directly from the source-derived data.

## Known Limitations

- Actions inside `[[topology.transitions]]` blocks are parsed heuristically; complex guard chains may not fully resolve to their correct targets.
- Messages from external `*-messages` crates are referenced but not deeply parsed yet.
- Action functions from `*-actions` crates are not yet extracted.
