// Copyright 2025 Bloxide, all rights reserved
//! [`bloxide_timer::TimerService`] for the browser / WASM runtime (same role as Tokio / Embassy).
//!
//! Time is read from [`Performance::now`] (monotonic, available on both `Window` and
//! [`WorkerGlobalScope`], including dedicated workers). Waits use `setTimeout` on the same global
//! (`Window` or worker scope), not `window` only — workers have no `window`.

use bloxide_core::messaging::Envelope;
use bloxide_timer::{TimerCommand, TimerQueue, TimerService};
use futures_util::select_biased;
use futures_util::stream::StreamExt;
use futures_util::FutureExt;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

use crate::channel::WasmStream;
use crate::WasmRuntime;

/// Browser global that exposes [`Performance`] and `setTimeout` (main thread or worker).
#[cfg(target_arch = "wasm32")]
enum JsTimeGlobal {
    Window(web_sys::Window),
    Worker(web_sys::WorkerGlobalScope),
}

#[cfg(target_arch = "wasm32")]
impl JsTimeGlobal {
    fn try_from_js_global() -> Option<Self> {
        let g = js_sys::global();
        if let Ok(w) = g.clone().dyn_into::<web_sys::Window>() {
            return Some(Self::Window(w));
        }
        if let Ok(dw) = g.dyn_into::<web_sys::DedicatedWorkerGlobalScope>() {
            return Some(Self::Worker(dw.unchecked_into()));
        }
        None
    }

    fn performance(&self) -> Option<web_sys::Performance> {
        match self {
            Self::Window(w) => w.performance(),
            Self::Worker(wgs) => wgs.performance(),
        }
    }

    fn set_timeout_with_callback_and_timeout_and_arguments_0(
        &self,
        callback: &js_sys::Function,
        timeout: i32,
    ) -> Result<i32, wasm_bindgen::JsValue> {
        match self {
            Self::Window(w) => {
                w.set_timeout_with_callback_and_timeout_and_arguments_0(callback, timeout)
            }
            Self::Worker(wgs) => {
                wgs.set_timeout_with_callback_and_timeout_and_arguments_0(callback, timeout)
            }
        }
    }
}

fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(global) = JsTimeGlobal::try_from_js_global() {
            if let Some(perf) = global.performance() {
                return perf.now() as u64;
            }
        }
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        let start = START.get_or_init(Instant::now);
        start.elapsed().as_millis() as u64
    }
}

/// Sleep for `ms` milliseconds (0 = microtask yield via a resolved promise).
async fn sleep_ms(ms: u32) {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::Promise;
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsValue;
        use wasm_bindgen_futures::JsFuture;

        if ms == 0 {
            let p = Promise::resolve(&JsValue::undefined());
            let _ = JsFuture::from(p).await;
            return;
        }

        let Some(global) = JsTimeGlobal::try_from_js_global() else {
            std::thread::sleep(std::time::Duration::from_millis(u64::from(ms)));
            return;
        };

        let ms_i32 = i32::try_from(ms).unwrap_or(i32::MAX);

        let promise = Promise::new(&mut |resolve, _reject| {
            let on_timer = resolve.clone();
            let on_fallback = resolve.clone();
            let closure = Closure::wrap(Box::new(move || {
                let _ = on_timer.call0(&JsValue::undefined());
            }) as Box<dyn FnMut()>);
            if global
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    closure.as_ref().unchecked_ref(),
                    ms_i32,
                )
                .is_err()
            {
                let _ = on_fallback.call0(&JsValue::undefined());
            }
            closure.forget();
        });

        let _ = JsFuture::from(promise).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::sleep(std::time::Duration::from_millis(u64::from(ms.max(1))));
    }
}

impl TimerService for WasmRuntime {
    async fn run_timer_service(stream: WasmStream<TimerCommand>) {
        let mut stream = stream.fuse();
        let mut queue = TimerQueue::new();
        loop {
            match queue.next_deadline() {
                Some(deadline_ms) => {
                    let now = now_ms();
                    let remaining_ms = deadline_ms.saturating_sub(now);
                    let wait_ms = (remaining_ms.min(u64::from(u32::MAX))) as u32;

                    select_biased! {
                        env = stream.next() => match env {
                            Some(Envelope(_, cmd)) => {
                                let now = now_ms();
                                if queue.handle_command(cmd, now) {
                                    for deliver in queue.drain_expired(now) {
                                        deliver();
                                    }
                                    return;
                                }
                                for deliver in queue.drain_expired(now) {
                                    deliver();
                                }
                            }
                            None => {
                                debug_assert!(false, "timer stream closed unexpectedly");
                                return;
                            }
                        },
                        _ = sleep_ms(wait_ms).fuse() => {
                            let now = now_ms();
                            for deliver in queue.drain_expired(now) {
                                deliver();
                            }
                        }
                    }
                }
                None => match stream.next().await {
                    Some(Envelope(_, cmd)) => {
                        let now = now_ms();
                        if queue.handle_command(cmd, now) {
                            for deliver in queue.drain_expired(now) {
                                deliver();
                            }
                            return;
                        }
                        for deliver in queue.drain_expired(now) {
                            deliver();
                        }
                    }
                    None => {
                        debug_assert!(false, "timer stream closed unexpectedly");
                        return;
                    }
                },
            }
        }
    }
}
