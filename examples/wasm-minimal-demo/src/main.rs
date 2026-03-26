// Copyright 2025 Bloxide, all rights reserved
//! Dioxus shell for a minimal Bloxide **main-thread** WASM binary (counter blox + timer).

mod layer5;

use bloxide_dioxus::BloxideWasmMainHost;
use dioxus::prelude::*;

fn main() {
    // Ensures `tracing` events go to the browser console via `tracing-wasm` (see `dioxus-logger`).
    // `dioxus::launch` also calls this if needed; calling here documents intent for Layer 5 logs.
    dioxus::logger::initialize_default();

    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        style { "{DEMO_CSS}" }
        div { class: "bdw-root",
            header { class: "bdw-header",
                h1 { "Bloxide WASM demo" }
                p { class: "bdw-sub",
                    "Counter blox on bloxide-wasm: timer service fires two domain ticks; actor stops at terminal state. Open the devtools console — look for INFO lines from target wasm_minimal_demo."
                }
            }
            BloxideWasmMainHost {
                start: move |_| {
                    layer5::run_counter_demo();
                },
            }
            section { class: "bdw-note",
                h2 { "Worker example (optional)" }
                p {
                    "For the dedicated-worker build, run "
                    code { "wasm-pack build integrations/bloxide-wasm-example --target web --out-dir …" }
                    " and use "
                    code { "BloxideWorkerHost" }
                    " with your served "
                    code { "worker_bootstrap.mjs" }
                    " URL — see "
                    code { "integrations/bloxide-wasm-example/README.md" }
                    "."
                }
            }
        }
    }
}

const DEMO_CSS: &str = r#"
:root { font-family: system-ui, sans-serif; }
body { margin: 0; background: #0f1419; color: #e6edf3; }
.bdw-root { padding: 20px 24px 32px; max-width: 52rem; }
.bdw-header h1 { margin: 0 0 8px; font-size: 1.4rem; }
.bdw-sub { margin: 0 0 20px; color: #8b9cad; line-height: 1.45; }
.bloxide-wasm-main-host {
  padding: 10px 14px;
  border-radius: 8px;
  background: #1a222c;
  border: 1px solid #2d3848;
  font-size: 0.95rem;
}
.bdw-note { margin-top: 28px; }
.bdw-note h2 { font-size: 1rem; margin: 0 0 8px; }
.bdw-note p { margin: 0; color: #8b9cad; line-height: 1.5; font-size: 0.9rem; }
.bdw-note code {
  font-size: 0.82rem;
  background: #1a222c;
  padding: 2px 6px;
  border-radius: 4px;
}
"#;
