// Copyright 2025 Bloxide, all rights reserved
//! Integration tests for the Pong blox.
//!
//! Blox-level tests verify state topology and guard logic only.
//! Action side effects (sending Pong messages) are tested at the
//! system level with concrete impl functions.
//!
//! Run with: `cargo blox generate`, then
//! `cargo test --manifest-path target/bloxide-generated/Cargo.toml -p pong-blox`

use bloxide_core::engine::DispatchOutcome;
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_core::messaging::Envelope;
use bloxide_core::{DynamicChannelCap, MachineState, StateMachine};
use bloxide_test_runtime::TestRuntime;
use ping_pong_messages::{Ping, PingPongMsg, Pong};
use pong_blox::{PongCtx, PongEvent, PongSpec, PongState};

struct PongHarness {
    machine: StateMachine<PongSpec<TestRuntime>>,
}

impl PongHarness {
    fn new() -> Self {
        let pong_id = TestRuntime::alloc_actor_id();
        let ping_id = TestRuntime::alloc_actor_id();
        let (ping_ref, _to_ping_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<PingPongMsg>(ping_id, 16);

        let ctx = PongCtx::new(pong_id, ping_ref);
        let machine = StateMachine::<PongSpec<TestRuntime>>::new(ctx);

        PongHarness { machine }
    }

    fn start(&mut self) {
        self.machine
            .dispatch(PongEvent::Lifecycle(LifecycleCommand::Start));
    }

    fn terminate(&mut self) {
        self.machine
            .dispatch(PongEvent::Lifecycle(LifecycleCommand::Reset));
    }

    fn send_ping(&mut self, n: u32) {
        self.machine.dispatch(PongEvent::Msg(Envelope(
            0,
            PingPongMsg::Ping(Ping { round: n }),
        )));
    }

    fn current_state(&self) -> MachineState<PongState> {
        self.machine.current_state()
    }
}

#[test]
fn start_enters_ready() {
    let mut h = PongHarness::new();
    h.start();

    assert_eq!(h.current_state(), MachineState::State(PongState::Ready));
}

#[test]
fn ping_in_ready_stays_in_ready() {
    // With stub actions, the state transition is tested but no
    // Pong message is actually sent. That's tested at system level.
    let mut h = PongHarness::new();
    h.start();

    h.send_ping(3);

    assert_eq!(
        h.current_state(),
        MachineState::State(PongState::Ready),
        "Pong must stay in Ready"
    );
}

#[test]
fn multiple_pings_stay_in_ready() {
    let mut h = PongHarness::new();
    h.start();

    for n in [1u32, 2, 4, 7] {
        h.send_ping(n);
    }

    assert_eq!(h.current_state(), MachineState::State(PongState::Ready));
}

#[test]
fn terminate_resets_to_initial_state() {
    let mut h = PongHarness::new();
    h.start();

    assert_eq!(h.current_state(), MachineState::State(PongState::Ready));

    h.terminate();

    // In the five-level lifecycle model, Reset goes directly to
    // initial_state() (Ready) — not Init. The machine is immediately
    // operational.
    assert_eq!(
        h.current_state(),
        MachineState::State(PongState::Ready),
        "machine must be in Ready (initial_state) after reset"
    );
}

#[test]
fn unhandled_event_bubbles_to_root_and_is_dropped() {
    let mut h = PongHarness::new();
    h.start();

    // Ready has a rule only for PingPongMsg::Ping — a stray Pong bubbles
    // to root, where no root rules exist.
    let outcome = h.machine.dispatch(PongEvent::Msg(Envelope(
        0,
        PingPongMsg::Pong(Pong { round: 3 }),
    )));
    assert_eq!(outcome, DispatchOutcome::NoRuleMatched);
    assert_eq!(
        h.current_state(),
        MachineState::State(PongState::Ready),
        "unhandled event must not change state"
    );

    // The machine still processes subsequent valid events.
    h.send_ping(1);
    assert_eq!(h.current_state(), MachineState::State(PongState::Ready));
}
