// Copyright 2025 Bloxide, all rights reserved
//! Dioxus integration for Bloxide **Layer 5** (binary / wiring) on WASM.
//!
//! - [`BloxideWasmMainHost`] runs **your** main-thread wiring once (full app: channels, machines,
//!   `spawn_task`, etc.).
//! - [`BloxideWorkerHost`] starts a **module worker** and completes the `MessagePort` handshake so
//!   your **worker** WASM binary can run its own full wiring inside
//!   `initBloxideApp` (see the `bloxide-wasm-example` crate README in this repo).
//!
//! Which actors/graph you run is determined by **what you pass in**: the `start` callback for the
//! main host, or **which worker script URL** you load (that script + its WASM is your “binary”).
//!
//! For workers that accept main-thread control traffic, use
//! [`BloxideWorkerHandle::post_message_str`] / [`post_message`](BloxideWorkerHandle::post_message)
//! until a typed `RemoteActorRef` exists. The stock `bloxide-wasm-example` worker does not need
//! inbound messages: it is event-driven inside the worker.

use std::ops::Deref;
use std::rc::Rc;

use dioxus::prelude::*;
use js_sys::{Array, Object, Reflect};
use wasm_bindgen::JsValue;
use web_sys::{MessageChannel, MessagePort, Worker, WorkerOptions, WorkerType};

/// Default `event.data.type` for the worker handshake (main → worker).
pub const HANDSHAKE_MESSAGE_TYPE: &str = "bloxide-start";
/// Default field on the handshake payload carrying the transferred [`MessagePort`].
pub const HANDSHAKE_PORT_FIELD: &str = "port";

struct BloxideWorkerInner {
    worker: Worker,
    port: MessagePort,
}

impl Drop for BloxideWorkerInner {
    fn drop(&mut self) {
        self.worker.terminate();
    }
}

/// Handle to a spawned worker and the main-thread [`MessagePort`] (peer of the port passed into
/// `initBloxideApp` in the worker).
#[derive(Clone)]
pub struct BloxideWorkerHandle {
    inner: Rc<BloxideWorkerInner>,
}

impl BloxideWorkerHandle {
    /// `postMessage` structured-clone payload (e.g. JSON via [`Self::post_message_str`]).
    pub fn post_message(&self, msg: &JsValue) -> Result<(), JsValue> {
        self.inner.port.post_message(msg)
    }

    /// Convenience for protocols that use a JSON **string** body (UTF-8).
    pub fn post_message_str(&self, json: &str) -> Result<(), JsValue> {
        self.post_message(&JsValue::from_str(json))
    }

    pub fn port(&self) -> &MessagePort {
        &self.inner.port
    }

    pub fn worker(&self) -> &Worker {
        &self.inner.worker
    }
}

/// Start a module worker and perform the default Bloxide handshake.
///
/// The worker’s bootstrap JS should `await init()` then call `initBloxideApp(event.data.port)` —
/// see `integrations/bloxide-wasm-example`.
pub fn spawn_bloxide_worker(worker_script_url: &str) -> Result<BloxideWorkerHandle, JsValue> {
    spawn_bloxide_worker_with_handshake(
        worker_script_url,
        HANDSHAKE_MESSAGE_TYPE,
        HANDSHAKE_PORT_FIELD,
    )
}

/// Same as [`spawn_bloxide_worker`] but allows custom handshake field names.
pub fn spawn_bloxide_worker_with_handshake(
    worker_script_url: &str,
    handshake_message_type: &str,
    handshake_port_field: &str,
) -> Result<BloxideWorkerHandle, JsValue> {
    let opts = WorkerOptions::new();
    WorkerOptions::set_type(&opts, WorkerType::Module);
    let worker = Worker::new_with_options(worker_script_url, &opts)?;

    let channel = MessageChannel::new()?;
    let init = Object::new();
    Reflect::set(
        &init,
        &JsValue::from_str("type"),
        &JsValue::from_str(handshake_message_type),
    )?;
    Reflect::set(
        &init,
        &JsValue::from_str(handshake_port_field),
        channel.port2().as_ref(),
    )?;

    let transfer = Array::new();
    transfer.push(channel.port2().as_ref());
    worker.post_message_with_transfer(&init.into(), &transfer)?;

    channel.port1().start();

    Ok(BloxideWorkerHandle {
        inner: Rc::new(BloxideWorkerInner {
            worker,
            port: channel.port1(),
        }),
    })
}

// ── Main-thread host (Layer 5 lives in `start`) ──────────────────────────────

#[derive(Props, PartialEq, Clone)]
pub struct BloxideWasmMainHostProps {
    /// Your **entire** main-thread binary wiring. For async work, call
    /// `wasm_bindgen_futures::spawn_local` (or `spawn_task` from `bloxide-wasm`) inside this hook.
    pub start: EventHandler<()>,
}

/// Runs **`start` once** when the component is first mounted — use this as the Dioxus entry for a
/// whole Bloxide app (same responsibility as `fn main` / `#[tokio::main]` in a native binary).
#[component]
pub fn BloxideWasmMainHost(props: BloxideWasmMainHostProps) -> Element {
    let start = props.start;
    use_effect(move || {
        start.call(());
    });

    rsx! {
        div { class: "bloxide-wasm-main-host", "Bloxide main (Layer 5) started" }
    }
}

// ── Worker host (worker binary = URL you pass in) ────────────────────────────

#[derive(Props, PartialEq, Clone)]
pub struct BloxideWorkerHostProps {
    /// URL to the worker **JavaScript** entry (ES module), not the `.wasm` file.
    pub worker_script_url: String,
    #[props(default = None)]
    pub handshake_message_type: Option<String>,
    #[props(default = None)]
    pub handshake_port_field: Option<String>,
}

/// Spawns **whatever worker binary** lives at `worker_script_url` (your full Layer 5 in the worker).
#[component]
pub fn BloxideWorkerHost(props: BloxideWorkerHostProps) -> Element {
    let url = props.worker_script_url.clone();
    let msg_ty = props.handshake_message_type.clone();
    let port_field = props.handshake_port_field.clone();

    let resource = use_resource(move || {
        let url = url.clone();
        let msg_ty = msg_ty.clone();
        let port_field = port_field.clone();
        async move {
            let ty = msg_ty
                .as_deref()
                .unwrap_or(HANDSHAKE_MESSAGE_TYPE);
            let pf = port_field
                .as_deref()
                .unwrap_or(HANDSHAKE_PORT_FIELD);
            spawn_bloxide_worker_with_handshake(&url, ty, pf)
        }
    });

    let status = match resource.state().read().deref() {
        UseResourceState::Pending => "starting…",
        UseResourceState::Ready => match resource.value().read().as_ref() {
            Some(Ok(_)) => "ready",
            Some(Err(_)) => "failed (see console)",
            None => "finished",
        },
        UseResourceState::Stopped => "stopped",
        UseResourceState::Paused => "paused",
    };

    rsx! {
        div { class: "bloxide-worker-host", "Bloxide worker binary: {status}" }
    }
}

/// Type alias for older examples; prefer [`BloxideWorkerHandle`].
pub type BloxideCounterWorkerHandle = BloxideWorkerHandle;

/// Deprecated name; use [`spawn_bloxide_worker`].
#[inline]
pub fn spawn_bloxide_counter_worker(worker_module_url: &str) -> Result<BloxideWorkerHandle, JsValue> {
    spawn_bloxide_worker(worker_module_url)
}

/// Deprecated; prefer [`BloxideWorkerHost`].
pub type BloxideCounterWorkerProps = BloxideWorkerHostProps;

/// Deprecated; prefer [`BloxideWorkerHost`].
#[component]
pub fn BloxideCounterWorker(props: BloxideCounterWorkerProps) -> Element {
    rsx! {
        BloxideWorkerHost {
            worker_script_url: props.worker_script_url,
            handshake_message_type: props.handshake_message_type,
            handshake_port_field: props.handshake_port_field,
        }
    }
}
