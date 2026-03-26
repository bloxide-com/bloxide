# bloxide-wasm-example

WASM `cdylib` **example**: the workspace **counter blox** on [`bloxide_wasm::WasmRuntime`] inside a
dedicated worker.

Domain **events** ([`CounterMsg::Tick`]) are injected from **this worker’s own async tasks** (here,
a small concurrent “event source” alongside `run_actor`), not from the main thread posting ticks.
That matches how real actors get work: timers, I/O, channels, etc.

The [`MessagePort`] from [`bloxide-dioxus`](../bloxide-dioxus/README.md) is still created for the
standard handshake; this example uses it only to **`post_message`** a JSON status when the actor
finishes (`{"status":"done"}`) — not as a control plane for domain messages.

## Build

```bash
rustup target add wasm32-unknown-unknown
wasm-pack build integrations/bloxide-wasm-example --target web --out-dir pkg
```

## Worker entry

After `wasm_bindgen` `init()`, bootstrap JS calls **`initBloxideApp(port)`** (see `js/worker_bootstrap.mjs`).

## Dioxus

Use [`BloxideWorkerHost`](../bloxide-dioxus/README.md) with a URL to `worker_bootstrap.mjs`. You do
not need to send ticks; listen on the port if you want the optional `done` status.
