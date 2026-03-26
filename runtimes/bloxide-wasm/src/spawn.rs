// Copyright 2025 Bloxide, all rights reserved
use core::future::Future;

/// Spawn a fire-and-forget task for actor loops and bridges.
///
/// On `wasm32-unknown-unknown` this uses [`wasm_bindgen_futures::spawn_local`]
/// (the future does not need to be [`Send`]). On host targets this uses a
/// background thread and [`pollster`] so `cargo check --workspace` works without
/// a WASM target.
#[cfg(target_arch = "wasm32")]
pub fn spawn_task(fut: impl Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(fut);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_task(fut: impl Future<Output = ()> + Send + 'static) {
    std::thread::spawn(move || {
        pollster::block_on(fut);
    });
}
