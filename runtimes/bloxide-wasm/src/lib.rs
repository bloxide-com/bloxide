// Copyright 2025 Bloxide, all rights reserved
//! Browser-oriented [`BloxRuntime`](bloxide_core::capability::BloxRuntime) for
//! Bloxide using [`async_channel`] and [`spawn_task`] (`spawn_local` on WASM).
//!
//! See `README.md` in this crate for a step-by-step integration guide.

extern crate alloc;

pub use bloxide_core::{run_actor, run_actor_auto_start, run_actor_to_completion};

#[doc(hidden)]
pub use bloxide_macros::dyn_channels as __dyn_channels_proc_macro;
#[doc(hidden)]
pub use bloxide_macros::next_actor_id as __next_actor_id_proc_macro;

pub mod channel;
pub mod mailbox;
pub mod prelude;
pub mod spawn;
pub mod timer;

pub use channel::{WasmSendError, WasmSender, WasmStream, WasmTrySendError};
pub use spawn::spawn_task;

// ── WasmRuntime ───────────────────────────────────────────────────────────────

/// Zero-sized [`bloxide_core::capability::BloxRuntime`] handle for browser WASM.
#[derive(Clone, Copy)]
pub struct WasmRuntime;

// ── next_actor_id! ────────────────────────────────────────────────────────────

#[macro_export]
macro_rules! next_actor_id {
    () => {
        $crate::__next_actor_id_proc_macro!()
    };
}

// ── channels! ─────────────────────────────────────────────────────────────────

#[macro_export]
macro_rules! channels {
    ($($tt:tt)*) => {
        $crate::__dyn_channels_proc_macro!($crate::WasmRuntime; $($tt)*)
    };
}

// ── actor_task! ───────────────────────────────────────────────────────────────

#[macro_export]
macro_rules! actor_task {
    ($name:ident, $spec:ty $(,)?) => {
        async fn $name(
            machine: ::bloxide_core::StateMachine<$spec>,
            mailboxes: <$spec as ::bloxide_core::spec::MachineSpec>::Mailboxes<
                $crate::WasmRuntime,
            >,
        ) {
            $crate::run_actor(machine, mailboxes).await;
        }
    };
}

// ── spawn_timer! ──────────────────────────────────────────────────────────────

/// Spawn the timer service and return the `ActorRef<TimerCommand>` for it (same pattern as
/// `bloxide-tokio` / `bloxide-embassy`).
#[macro_export]
macro_rules! spawn_timer {
    ($capacity:expr) => {{
        let ((timer_ref,), (timer_stream,)) =
            $crate::__dyn_channels_proc_macro!($crate::WasmRuntime; ::bloxide_timer::TimerCommand($capacity));
        $crate::spawn_task(
            <$crate::WasmRuntime as ::bloxide_timer::TimerService>::run_timer_service(timer_stream),
        );
        timer_ref
    }};
}
