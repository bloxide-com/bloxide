# bloxide-dioxus

Dioxus hooks for starting **Layer 5** (your wiring / “binary”) on WASM:

- **`BloxideWasmMainHost`** — you pass an `EventHandler` that *is* your main-thread app setup (same role as `main` in a native crate).
- **`BloxideWorkerHost`** — you pass the URL of a **worker JS entry**; that worker’s WASM + `initBloxideApp` runs *its* full Layer 5 (possibly many bloxes, supervisors, etc.).

Which graph runs is entirely **what you implement** in that callback or in that worker artifact — not something this crate hardcodes.

## Main thread: whole Bloxide app

```rust, ignore
use dioxus::prelude::*;
use bloxide_dioxus::BloxideWasmMainHost;
use wasm_bindgen_futures::spawn_local;

#[component]
fn App() -> Element {
    rsx! {
        BloxideWasmMainHost {
            start: |_| {
                spawn_local(async move {
                    my_wasm_binary::run_layer5().await;
                });
            },
        }
    }
}
```

`my_wasm_binary::run_layer5` is where you put `channels!`, `StateMachine::new`, `bloxide_wasm::spawn_task`, etc. — the same code you would have put in `fn main` for a WASM binary without Dioxus.

## Worker: whole Bloxide binary in a dedicated worker

1. Build your worker `cdylib` with wasm-pack (see `integrations/bloxide-wasm-example` — it runs the real **counter blox** as a sample).
2. Export **`initBloxideApp(MessagePort)`** from Rust (`#[wasm_bindgen(js_name = initBloxideApp)]`) and run your **entire** wiring there.
3. Bootstrap JS: `await init()` then `initBloxideApp(event.data.port)` (see `integrations/bloxide-wasm-example/js/worker_bootstrap.mjs`).
4. In Dioxus:

```rust, ignore
use bloxide_dioxus::BloxideWorkerHost;

rsx! {
    BloxideWorkerHost {
        worker_script_url: "/assets/worker_bootstrap.mjs".to_string(),
    }
}
```

For workers that **do** accept main-thread control messages, obtain a [`BloxideWorkerHandle`] with [`spawn_bloxide_worker`] and use [`post_message_str`](BloxideWorkerHandle::post_message_str) / [`post_message`](BloxideWorkerHandle::post_message). The stock [`bloxide-wasm-example`](../../integrations/bloxide-wasm-example/README.md) does **not** require that: it drives domain events inside the worker and only posts an optional status JSON on completion.

Handshake defaults: message `type` = `"bloxide-start"`, port field = `"port"`. Override via `BloxideWorkerHost` optional props `handshake_message_type` / `handshake_port_field` if needed.

## Deprecated names

`BloxideCounterWorker`, `spawn_bloxide_counter_worker`, and `BloxideCounterWorkerHandle` remain as aliases for older snippets; prefer the generic names above.
