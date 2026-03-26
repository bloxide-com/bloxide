# bloxide-wasm — browser `BloxRuntime`

This crate implements [`BloxRuntime`](../../crates/bloxide-core/src/capability.rs) and
[`DynamicChannelCap`](../../crates/bloxide-core/src/capability.rs) using a bounded
[`async_channel`] and [`spawn_task`](src/spawn.rs) (`wasm_bindgen_futures::spawn_local` on
`wasm32-unknown-unknown`, a small thread + `pollster` shim on host targets so `cargo check
--workspace` works).

## Step-by-step: add the runtime to a WASM app

1. **Depend on the crate** (path or crates.io once published):

   `bloxide-wasm = { path = "../runtimes/bloxide-wasm" }` (adjust path).

2. **Pick the same blox stack as other runtimes** — e.g. `counter-blox` with
   `CounterSpec<WasmRuntime, YourBehavior>`.

3. **Create mailboxes** with the `channels!` macro (same shape as Tokio):

   ```rust
   let ((self_ref,), (stream,)) = bloxide_wasm::channels! { CounterMsg(16) };
   ```

4. **Build the `StateMachine` and run it** on a task:

   ```rust
   bloxide_wasm::spawn_task(async move {
       bloxide_core::run_actor(machine, (stream,)).await;
   });
   ```

5. **On `wasm32`**, ensure the binary / worker entry calls `spawn_task` from an async context
   already driven by the JS event loop (e.g. `#[wasm_bindgen(start)]` that uses
   `spawn_local`, or a `MessagePort` bridge as in `bloxide-wasm-example`).

6. **Timers / supervision** — not wired in this first version; add a `TimerService` impl and
   supervision channels the same way `bloxide-tokio` / `bloxide-embassy` do when you need them.

## Web Worker example

See [`integrations/bloxide-wasm-example`](../integrations/bloxide-wasm-example/)
for a `cdylib` that runs the workspace **counter blox** inside a worker; domain events are driven
in-process (not via UI “ticks” on the port).

## Dioxus companion

[`integrations/bloxide-dioxus`](../integrations/bloxide-dioxus/) provides `BloxideWasmMainHost`
(main-thread Layer 5 via a `start` callback) and `BloxideWorkerHost` (spawn **your** worker
binary by URL + `initBloxideApp` handshake). Wire format to the worker is up to each binary.
