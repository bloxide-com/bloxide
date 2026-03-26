// Copyright 2025 Bloxide, all rights reserved
//! WASM **worker** example: the shipped [`counter_blox`](counter_blox) layered demo on
//! [`bloxide_wasm::WasmRuntime`]. Domain **events** are produced inside the worker (concurrent with
//! the actor loop), not by the UI “ticking” the blox over [`MessagePort`].
//!
//! The transferred port is only used to signal completion to the main thread (optional wire
//! payload); inbound tick JSON is intentionally not part of this example.
//!
//! Build: `wasm-pack build integrations/bloxide-wasm-example --target web --out-dir pkg`

use bloxide_core::{run_actor, StateMachine};
use bloxide_wasm::spawn_task;
use counter_blox::{CounterCtx, CounterSpec};
use counter_demo_impl::CounterBehavior;
use counter_messages::{CounterMsg, Tick};
use futures_util::future::join;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::MessagePort;

/// Synthetic [`bloxide_core::ActorId`] used when this worker injects [`CounterMsg`] from its own
/// async tasks (same pattern as timers, I/O callbacks, or other event sources).
const INTERNAL_EVENT_SOURCE: usize = 0;

/// Start this worker’s Layer 5 after `wasm_bindgen` `init()`.
///
/// `port` is the [`MessagePort`] from the `bloxide-dioxus` worker handshake: it is **started** here and,
/// after the counter reaches its terminal state, receives a small JSON status message (not a
/// domain event — observability only).
#[wasm_bindgen(js_name = initBloxideApp)]
pub fn init_bloxide_app(port: MessagePort) {
    let ((actor_ref,), (stream,)) = bloxide_wasm::channels! {
        CounterMsg(16),
    };
    let id = actor_ref.id();
    let event_source = actor_ref.clone();

    let machine = StateMachine::<CounterSpec<bloxide_wasm::WasmRuntime, CounterBehavior>>::new(
        CounterCtx::new(id, CounterBehavior::default()),
    );

    port.start();

    spawn_task(async move {
        join(
            run_actor(machine, (stream,)),
            async move {
                // Drive the machine with ordinary domain events from this worker’s event loop,
                // not from manual UI / postMessage ticks.
                let _ = event_source
                    .send(INTERNAL_EVENT_SOURCE, CounterMsg::Tick(Tick {}))
                    .await;
                let _ = event_source
                    .send(INTERNAL_EVENT_SOURCE, CounterMsg::Tick(Tick {}))
                    .await;
            },
        )
        .await;
        let _ = port.post_message(&JsValue::from_str(r#"{"status":"done"}"#));
    });
}
