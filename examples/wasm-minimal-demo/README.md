# wasm-minimal-demo

Dioxus **web** app that runs a real Bloxide **Layer 5** graph on the main thread — the browser counterpart to [`examples/tokio-minimal-demo.rs`](../tokio-minimal-demo.rs):

- [`bloxide_wasm::spawn_timer!`](../../runtimes/bloxide-wasm/src/lib.rs) + [`TimerService`](../../crates/bloxide-timer/src/service.rs)
- **Counter** blox (`counter-blox` + `counter-demo-impl`), driven by two timer-delivered [`CounterMsg::Tick`](../../crates/messages/counter-messages/src/lib.rs) events

The UI shell uses [`BloxideWasmMainHost`](../../integrations/bloxide-dioxus/src/lib.rs) so the wiring runs once on mount (same role as `main` in a native binary).

## Run (Dioxus CLI)

Install the CLI if needed (`cargo install dioxus-cli` — use a version compatible with Dioxus 0.7).

From the **repository root** (same pattern as `cargo run --example tokio-minimal-demo`, but the web stack needs `dx`):

```bash
dx serve -p wasm-minimal-demo
```

Open the URL the CLI prints (usually `http://127.0.0.1:8080`). Open the browser **devtools console**: Dioxus installs `tracing-wasm`, and this demo emits **`INFO`** lines with target **`wasm_minimal_demo`** (wiring, timer callbacks, counter actor start/finish). You can filter the console by that string.

## Build static output

```bash
dx build -p wasm-minimal-demo
```

Artifacts go under `dist/` per `Dioxus.toml`.

## Related

- Dedicated **worker** WASM (`initBloxideApp`): [`integrations/bloxide-wasm-example`](../../integrations/bloxide-wasm-example/README.md) + [`BloxideWorkerHost`](../../integrations/bloxide-dioxus/src/lib.rs).
