// Copyright 2025 Bloxide, all rights reserved
//! Tests for the generated supervisor state machine.
//!
//! These tests were moved from the old hand-written `supervisor.rs` and
//! adapted to use the generated `SupervisorCtx` (which has a `child_notify`
//! field and a `new()` constructor).

extern crate alloc;
use alloc::vec::Vec;

use crate::RegisterDynamicChild;
use crate::{
    control::{RegisterChild, SupervisorControl},
    SupervisorCtx, SupervisorEvent, SupervisorSpec, SupervisorState,
};
use bloxide_child_management::{ChildGroup, ChildPolicy, GroupShutdown};
use bloxide_core::lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::Envelope;
use bloxide_core::{
    capability::DynamicChannelCap, engine::DispatchOutcome, engine::MachineState, StateMachine,
};
use bloxide_test_runtime::{TestReceiver, TestRuntime};

type Spec = SupervisorSpec<TestRuntime>;

fn make_supervisor(
    shutdown: GroupShutdown,
    policies: &[ChildPolicy],
) -> (StateMachine<Spec>, Vec<TestReceiver<LifecycleCommand>>) {
    let mut group = ChildGroup::new(shutdown);
    let mut receivers = Vec::new();
    for (i, policy) in policies.iter().enumerate() {
        let id = i + 1;
        let (actor_ref, rx) = TestRuntime::channel::<LifecycleCommand>(id, 16);
        group.add(id, actor_ref, *policy);
        receivers.push(rx);
    }
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(100, 16);
    let ctx = SupervisorCtx::new(100, group, notify_ref);
    (StateMachine::new(ctx), receivers)
}

fn dispatch_child_event(
    machine: &mut StateMachine<Spec>,
    event: ChildLifecycleEvent,
) -> DispatchOutcome<SupervisorState> {
    let ev = SupervisorEvent::<TestRuntime>::Child(Envelope(0, event));
    machine.dispatch(ev)
}

fn dispatch_control_event(
    machine: &mut StateMachine<Spec>,
    event: SupervisorControl<TestRuntime>,
) -> DispatchOutcome<SupervisorState> {
    let ev = SupervisorEvent::<TestRuntime>::Control(Envelope(0, event));
    machine.dispatch(ev)
}

fn drain_start_commands(receivers: &mut [TestReceiver<LifecycleCommand>]) {
    for rx in receivers.iter_mut() {
        let cmds = rx.drain_payloads();
        assert!(
            cmds.iter().all(|c| matches!(c, LifecycleCommand::Start)),
            "expected only Start commands, got {:?}",
            cmds,
        );
    }
}

#[test]
fn start_enters_running() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    let outcome = machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    assert_eq!(
        outcome,
        DispatchOutcome::Started(MachineState::State(SupervisorState::Running))
    );

    for rx in receivers.iter_mut() {
        let cmds = rx.drain_payloads();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], LifecycleCommand::Start));
    }
}

#[test]
fn restart_policy_stays_running_on_done() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Reset],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let cmds = receivers[0].drain_payloads();
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds[0], LifecycleCommand::Reset));
}

/// In the four-level lifecycle model, Reset goes directly to initial_state()
/// and returns Started. The supervisor does NOT send a separate Start after
/// Reset — the Reset command itself re-enters initial_state(). The supervisor
/// sees Started from the child (which is the outcome of the Reset dispatch).
#[test]
fn restart_policy_reset_returns_started_no_separate_start() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Reset],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child reports Stopped → supervisor sends Reset
    dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    receivers[0].drain_payloads();

    // Child reports Started (outcome of Reset going to initial_state)
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Started { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    // No additional commands should be sent — Reset is self-contained
    let cmds = receivers[0].drain_payloads();
    assert!(
        cmds.is_empty(),
        "no Start should be sent after Reset — Reset goes directly to initial_state()"
    );
}

#[test]
fn stop_policy_transitions_to_shutting_down() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    );
}

#[test]
fn shutting_down_stops_all_and_completes_when_done() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });

    for rx in receivers.iter_mut() {
        let cmds = rx.drain_payloads();
        assert!(
            cmds.iter().any(|c| matches!(c, LifecycleCommand::Stop)),
            "expected Stop command, got {:?}",
            cmds,
        );
    }

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    // When all children are stopped, the guard returns Guard::Stop — the
    // supervisor stops itself (goes to Init, reports Stopped). run_root
    // sees DispatchOutcome::Stopped and exits.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
}

#[test]
fn when_all_done_waits_for_all_children() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 2 });
    assert_eq!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    );
}

#[test]
fn stray_events_absorbed_in_running() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Started { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Alive { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
}

#[test]
fn stray_events_absorbed_in_shutting_down() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    for rx in receivers.iter_mut() {
        rx.drain_payloads();
    }

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Started { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Alive { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
}

#[test]
fn shutdown_completes_when_single_child_stops() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    for rx in receivers.iter_mut() {
        rx.drain_payloads();
    }

    // When the child stops, all children are stopped → guard returns
    // Guard::Stop — the supervisor stops itself (goes to Init, reports
    // Stopped). run_root sees DispatchOutcome::Stopped and exits.
    // Note: after Guard::Stop fires, on_init_entry calls clear_counters(),
    // so we cannot assert all_stopped() here — the DispatchOutcome::Stopped
    // is the proof that the guard fired.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
}

#[test]
fn failed_event_treated_same_as_done() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Reset],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Failed { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let cmds = receivers[0].drain_payloads();
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds[0], LifecycleCommand::Reset));
}

#[test]
fn register_child_event_adds_child_and_sends_start() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let child_id = 77usize;
    let (lifecycle_ref, mut lifecycle_rx) = TestRuntime::channel::<LifecycleCommand>(child_id, 8);
    let register = RegisterChild::<TestRuntime> {
        id: child_id,
        lifecycle_ref,
        policy: ChildPolicy::Stop,
    };

    let outcome = dispatch_control_event(&mut machine, SupervisorControl::RegisterChild(register));
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let cmds = lifecycle_rx.drain_payloads();
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds[0], LifecycleCommand::Start));
}

#[test]
fn health_check_tick_marks_unresponsive_restart_child_and_sends_ping() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Reset],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Tick #1: ping all monitored children.
    let outcome = dispatch_control_event(&mut machine, SupervisorControl::HealthCheckTick);
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
    let first = receivers[0].drain_payloads();
    assert_eq!(first.len(), 1);
    assert!(matches!(first[0], LifecycleCommand::Ping));

    // Tick #2 with no Alive from child:
    // stale child is handled as failure (Reset), then re-pinged.
    let outcome = dispatch_control_event(&mut machine, SupervisorControl::HealthCheckTick);
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
    let second = receivers[0].drain_payloads();
    assert_eq!(second.len(), 1);
    assert!(matches!(second[0], LifecycleCommand::Reset));
}

// ──────────────────────────────────────────────────────────────
// Abort lifecycle tests
// ──────────────────────────────────────────────────────────────

#[test]
fn aborted_child_marked_permanently_done() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[
            ChildPolicy::Reset,
            ChildPolicy::Reset,
        ],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child 1 aborts (cooperative task termination)
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Aborted { child_id: 1 });
    // Aborted children are permanently done — if all children are done/aborted,
    // the supervisor may transition to ShuttingDown or stay in Running depending
    // on the shutdown strategy. With WhenAllDone, one aborted + one running
    // means we stay in Running.
    assert!(
        matches!(outcome, DispatchOutcome::HandledNoTransition),
        "aborted child with WhenAllDone and one still running should stay in Running"
    );

    // Child 1 should not be restarted — it's permanently done
    let cmds = receivers[0].drain_payloads();
    assert!(
        cmds.is_empty(),
        "aborted child should not receive any commands"
    );
}

#[test]
fn aborted_child_does_not_trigger_shutdown_check() {
    // Aborted children are marked permanently_done, but the Aborted event
    // does not trigger the shutdown check (only Stopped/Failed do). This is
    // by design — Abort is cooperative termination, not a lifecycle event
    // that should cascade to shutdown.
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[
            ChildPolicy::Reset,
            ChildPolicy::Reset,
        ],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child 1 stopped, child 2 aborted — both are "done" but Aborted doesn't
    // trigger the shutdown check, so the supervisor stays in Running.
    dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Aborted { child_id: 2 });
    assert!(
        matches!(outcome, DispatchOutcome::HandledNoTransition),
        "Aborted event should not trigger shutdown transition"
    );
}

// ──────────────────────────────────────────────────────────────
// Dynamic child registration tests
// ──────────────────────────────────────────────────────────────

#[test]
fn register_dynamic_child_adds_and_starts() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Reset],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Create a new child channel for the dynamic child
    let child_id = 42;
    let (lifecycle_ref, mut lifecycle_rx) = TestRuntime::channel::<LifecycleCommand>(child_id, 16);
    let (abort_ref, _abort_rx) = TestRuntime::channel::<AbortCommand>(child_id + 100, 16);

    let reg = RegisterDynamicChild {
        id: child_id,
        lifecycle_ref,
        abort_ref,
        kill_handle: (),
        policy: ChildPolicy::Reset,
    };

    let outcome =
        dispatch_control_event(&mut machine, SupervisorControl::RegisterDynamicChild(reg));

    // Dynamic registration is handled in Running state (no state transition)
    assert!(
        matches!(outcome, DispatchOutcome::HandledNoTransition),
        "dynamic child registration should not cause state transition"
    );

    // The new child should receive a Start command
    let cmds = lifecycle_rx.drain_payloads();
    assert_eq!(
        cmds.len(),
        1,
        "new dynamic child should receive exactly one Start"
    );
    assert!(matches!(cmds[0], LifecycleCommand::Start));
}

#[test]
fn register_dynamic_child_during_shutdown_still_starts_child() {
    // NOTE: The current implementation sends Start to dynamically registered
    // children even during ShuttingDown. This is a known limitation —
    // ideally, registration during shutdown should be absorbed without
    // starting the child. This test documents the current behavior.
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Reset],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child 1 reports Stopped → triggers shutdown (WhenAnyDone)
    dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });

    // In ShuttingDown state, dynamic child registration is handled and
    // the child receives a Start (current behavior).
    let child_id = 99;
    let (lifecycle_ref, mut lifecycle_rx) = TestRuntime::channel::<LifecycleCommand>(child_id, 16);
    let (abort_ref, _abort_rx) = TestRuntime::channel::<AbortCommand>(child_id + 100, 16);

    let reg = RegisterDynamicChild {
        id: child_id,
        lifecycle_ref,
        abort_ref,
        kill_handle: (),
        policy: ChildPolicy::Reset,
    };

    let outcome =
        dispatch_control_event(&mut machine, SupervisorControl::RegisterDynamicChild(reg));

    assert!(
        matches!(outcome, DispatchOutcome::HandledNoTransition),
        "dynamic child registration should not cause state transition"
    );

    // Current behavior: child receives Start even during shutdown
    let cmds = lifecycle_rx.drain_payloads();
    assert_eq!(
        cmds.len(),
        1,
        "child receives Start (current behavior during shutdown)"
    );
    assert!(matches!(cmds[0], LifecycleCommand::Start));
}
