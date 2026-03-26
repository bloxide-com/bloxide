// Copyright 2025 Bloxide, all rights reserved
//! Layer 5 wiring for the web demo: **counter blox** + [`bloxide_timer`] service on
//! [`bloxide_wasm::WasmRuntime`]. Timer callbacks deliver [`CounterMsg::Tick`] (same pattern as
//! Tokio / Embassy, without UI “ticks”).

use bloxide_core::{run_actor_auto_start, StateMachine};
use bloxide_timer::{next_timer_id, TimerCommand};
use bloxide_wasm::spawn_task;
use counter_blox::{CounterCtx, CounterSpec};
use counter_demo_impl::CounterBehavior;
use counter_messages::{CounterMsg, Tick};
use tracing::info;

const WIRING_SENDER_ID: usize = 0;
const LOG_TARGET: &str = "wasm_minimal_demo";

/// Start timer service, counter actor, and two delayed ticks.
pub fn run_counter_demo() {
    info!(
        target: LOG_TARGET,
        "wiring: spawn timer service + counter mailbox (bloxide-wasm Layer 5)"
    );

    let timer_ref = bloxide_wasm::spawn_timer!(8);

    let ((counter_ref,), (stream,)) = bloxide_wasm::channels! {
        CounterMsg(16),
    };
    let actor_id = counter_ref.id();

    // Hold sender clones for the process lifetime: dropping the last sender closes the channel
    // while timer / actor tasks still poll — that trips `debug_assert!` in mailboxes and the timer
    // service (see `bloxide_core::mailboxes` channel invariant).
    let _keepalive = Box::leak(Box::new((timer_ref.clone(), counter_ref.clone())));

    let c1 = counter_ref.clone();
    let id1 = next_timer_id();
    timer_ref
        .try_send(
            WIRING_SENDER_ID,
            TimerCommand::Set {
                id: id1,
                after_ms: 400,
                deliver: Box::new(move || {
                    info!(target: LOG_TARGET, "timer: firing first Tick (400ms)");
                    let _ = c1.try_send(WIRING_SENDER_ID, CounterMsg::Tick(Tick {}));
                }),
            },
        )
        .ok();

    let c2 = counter_ref.clone();
    let id2 = next_timer_id();
    timer_ref
        .try_send(
            WIRING_SENDER_ID,
            TimerCommand::Set {
                id: id2,
                after_ms: 800,
                deliver: Box::new(move || {
                    info!(target: LOG_TARGET, "timer: firing second Tick (800ms)");
                    let _ = c2.try_send(WIRING_SENDER_ID, CounterMsg::Tick(Tick {}));
                }),
            },
        )
        .ok();

    info!(
        target: LOG_TARGET,
        "timer: scheduled two ticks (actor_id={actor_id})"
    );

    let machine = StateMachine::<CounterSpec<bloxide_wasm::WasmRuntime, CounterBehavior>>::new(
        CounterCtx::new(actor_id, CounterBehavior::default()),
    );

    spawn_task(async move {
        info!(
            target: LOG_TARGET,
            "counter actor: run_actor_auto_start (Start + mailbox loop)"
        );
        run_actor_auto_start(machine, (stream,)).await;
        info!(
            target: LOG_TARGET,
            "counter actor: exited (terminal Done — two ticks processed)"
        );
    });
}
