// Copyright 2025 Bloxide, all rights reserved
//! Tests for the generated supervisor state machine.
//!
//! These tests were moved from the old hand-written `supervisor.rs` and
//! adapted to use the generated `SupervisorCtx` (which has a `child_notify`
//! field and a `new()` constructor).

extern crate alloc;
use alloc::vec::Vec;

use crate::concrete_spec::ConcreteSupervisorSpec;
use crate::{SupervisorCtx, SupervisorEvent, SupervisorState};
use bloxide_child_management::{
    ChildCtrl, ChildGroup, ChildPolicy, GroupShutdown, RegisterChild, RegisterDynamicChild,
};
use bloxide_core::lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::Envelope;
use bloxide_core::{
    capability::DynamicChannelCap, engine::DispatchOutcome, engine::MachineState, StateMachine,
};
use bloxide_test_runtime::{TestReceiver, TestRuntime};

type Spec = ConcreteSupervisorSpec<TestRuntime>;

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
    event: ChildCtrl<TestRuntime>,
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
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset]);
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
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset]);
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

    // When all children are stopped, the guard returns Decision::Stop — the
    // supervisor stops itself (goes to Init, reports Stopped). The root run
    // loop (`run()` + `RunConfig::root()`) sees DispatchOutcome::Stopped and
    // exits.
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
    // Decision::Stop — the supervisor stops itself (goes to Init, reports
    // Stopped). The root run loop (`run()` + `RunConfig::root()`) sees
    // DispatchOutcome::Stopped and exits.
    // Note: after Decision::Stop fires, on_init_entry calls clear_counters(),
    // so we cannot assert all_stopped() here — the DispatchOutcome::Stopped
    // is the proof that the guard fired.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
}

#[test]
fn failed_event_treated_same_as_done() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset]);
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

    let outcome = dispatch_control_event(&mut machine, ChildCtrl::RegisterChild(register));
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let cmds = lifecycle_rx.drain_payloads();
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds[0], LifecycleCommand::Start));
}

#[test]
fn health_check_tick_marks_unresponsive_restart_child_and_sends_ping() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Tick #1: ping all monitored children.
    let outcome = dispatch_control_event(&mut machine, ChildCtrl::HealthCheckTick);
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
    let first = receivers[0].drain_payloads();
    assert_eq!(first.len(), 1);
    assert!(matches!(first[0], LifecycleCommand::Ping));

    // Tick #2 with no Alive from child:
    // stale child is handled as failure (Reset), then re-pinged.
    let outcome = dispatch_control_event(&mut machine, ChildCtrl::HealthCheckTick);
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
    let second = receivers[0].drain_payloads();
    assert_eq!(second.len(), 1);
    assert!(matches!(second[0], LifecycleCommand::Reset));
}

// ──────────────────────────────────────────────────────────────
// Abort lifecycle tests
// ──────────────────────────────────────────────────────────────

#[test]
fn aborted_child_marked_aborted() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Reset, ChildPolicy::Reset],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child 1 aborts (cooperative task termination)
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Aborted { child_id: 1 });
    // Aborted children are terminal (task gone) — if all children are
    // terminal, the supervisor may transition to ShuttingDown or stay in
    // Running depending on the shutdown strategy. With WhenAllDone, one
    // aborted + one running means we stay in Running.
    assert!(
        matches!(outcome, DispatchOutcome::HandledNoTransition),
        "aborted child with WhenAllDone and one still running should stay in Running"
    );

    // Child 1 should not be restarted — it's terminal
    let cmds = receivers[0].drain_payloads();
    assert!(
        cmds.is_empty(),
        "aborted child should not receive any commands"
    );
}

#[test]
fn killed_child_marked_killed() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Reset, ChildPolicy::Reset],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child 1 is killed (external task destruction)
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Killed { child_id: 1 });
    // Killed children are terminal (task gone, permanently dead) — with
    // WhenAllDone and one child still running, the supervisor stays Running.
    assert!(
        matches!(outcome, DispatchOutcome::HandledNoTransition),
        "killed child with WhenAllDone and one still running should stay in Running"
    );

    // Child 1 should not be restarted — it's terminal
    let cmds = receivers[0].drain_payloads();
    assert!(
        cmds.is_empty(),
        "killed child should not receive any commands"
    );
}

#[test]
fn aborted_child_does_not_trigger_shutdown_check() {
    // Aborted children are terminal, but the Aborted event
    // does not trigger the shutdown check (only Stopped/Failed do). This is
    // by design — Abort is cooperative termination, not a lifecycle event
    // that should cascade to shutdown.
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Reset, ChildPolicy::Reset],
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
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAllDone, &[ChildPolicy::Reset]);
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

    let outcome = dispatch_control_event(&mut machine, ChildCtrl::RegisterDynamicChild(reg));

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
fn register_dynamic_child_during_shutdown_is_absorbed() {
    // Registration during ShuttingDown is correctly absorbed by the Control
    // catch-all in SHUTTING_DOWN_FNS: the child is NOT registered and NOT
    // started. (The previous version of this test — "still_starts_child" —
    // claimed the child was started during shutdown, but its setup used
    // WhenAnyDone + ChildPolicy::Reset, so the supervisor never actually left
    // Running.)
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child 1 reports Stopped with Stop policy → WhenAnyDone → ShuttingDown.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert!(matches!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    ));
    // Drain the Stop sent by ShuttingDown's on_entry (stop_all_children).
    for rx in receivers.iter_mut() {
        rx.drain_payloads();
    }

    // In ShuttingDown, a RegisterDynamicChild control message hits the
    // Control catch-all and is absorbed without running any action.
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

    let outcome = dispatch_control_event(&mut machine, ChildCtrl::RegisterDynamicChild(reg));
    assert!(
        matches!(outcome, DispatchOutcome::HandledNoTransition),
        "dynamic child registration in ShuttingDown must be absorbed"
    );
    assert!(matches!(
        machine.current_state(),
        MachineState::State(SupervisorState::ShuttingDown)
    ));

    let cmds = lifecycle_rx.drain_payloads();
    assert!(
        cmds.is_empty(),
        "no Start may be sent during ShuttingDown, got {:?}",
        cmds
    );
}

// ──────────────────────────────────────────────────────────────
// Done (clean self-termination) tests
// ──────────────────────────────────────────────────────────────

#[test]
fn done_deregisters_child_without_restart() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child reports Done (clean completion). WhenAnyDone → shutdown begins.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    assert!(matches!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    ));

    // No Reset is sent — Done deregisters, it is not a fault. The child was
    // removed from the group, so ShuttingDown's stop_all sends nothing either.
    let cmds = receivers[0].drain_payloads();
    assert!(
        cmds.is_empty(),
        "Done must not trigger restart/stop, got {:?}",
        cmds
    );
}

#[test]
fn done_last_child_completes_group_shutdown() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // First Done: deregistered, but child 2 remains → still Running.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    // Second Done: last child deregistered → ShuttingDown.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 2 });
    assert!(matches!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    ));
}

// ──────────────────────────────────────────────────────────────
// concrete_spec ↔ generated topology equivalence
// ──────────────────────────────────────────────────────────────

/// `concrete_spec.rs` is a hand-written duplicate of the generated topology
/// (`generated/spec_skeleton.rs`). It exists because in-crate tests need
/// concrete action closures — the blox-level generated spec only carries stub
/// actions (invariant #18), and the system-level concrete spec lives in the
/// app crate. This test pins the hand-written copy to the generated spec so
/// the two cannot drift: same state set, same parents, same transition count
/// per state, same entry/exit action counts.
#[test]
fn concrete_spec_matches_generated_topology() {
    use bloxide_core::spec::MachineSpec;
    use bloxide_core::topology::StateTopology;

    type Generated = crate::SupervisorSpec<TestRuntime>;

    // Same state set: both specs share the generated `SupervisorState` enum.
    // If a variant is ever added, this array must be updated too.
    const ALL_STATES: [SupervisorState; 2] =
        [SupervisorState::Running, SupervisorState::ShuttingDown];
    assert_eq!(ALL_STATES.len(), SupervisorState::STATE_COUNT);
    assert_eq!(
        <Spec as MachineSpec>::initial_state(),
        <Generated as MachineSpec>::initial_state()
    );

    // Same parents: the supervisor topology is flat — every state is
    // top-level (parent = None) and a leaf.
    for state in ALL_STATES {
        assert_eq!(state.parent(), None, "parent mismatch at {:?}", state);
        assert!(state.is_leaf(), "{:?} must be a leaf", state);
        assert_eq!(state.path(), &[state], "path mismatch at {:?}", state);
    }

    // Both handler tables cover exactly the state set.
    let concrete_table = <Spec as MachineSpec>::HANDLER_TABLE;
    let generated_table = <Generated as MachineSpec>::HANDLER_TABLE;
    assert_eq!(concrete_table.len(), SupervisorState::STATE_COUNT);
    assert_eq!(concrete_table.len(), generated_table.len());

    for state in ALL_STATES {
        let idx = state.as_index();
        let concrete = concrete_table[idx];
        let generated = generated_table[idx];

        // Same entry/exit action counts per state.
        assert_eq!(
            concrete.on_entry.len(),
            generated.on_entry.len(),
            "on_entry action count mismatch at {:?}",
            state
        );
        assert_eq!(
            concrete.on_exit.len(),
            generated.on_exit.len(),
            "on_exit action count mismatch at {:?}",
            state
        );

        // Same transition count per state; each rule agrees on event tag and
        // action count (the concrete rule's action is the real platform
        // function, the generated rule's is a stub — count, not identity).
        assert_eq!(
            concrete.transitions.len(),
            generated.transitions.len(),
            "transition count mismatch at {:?}",
            state
        );
        for (i, (cr, gr)) in concrete
            .transitions
            .iter()
            .zip(generated.transitions.iter())
            .enumerate()
        {
            assert_eq!(
                cr.event_tag, gr.event_tag,
                "rule {} event_tag mismatch at {:?}",
                i, state
            );
            assert_eq!(
                cr.actions.len(),
                gr.actions.len(),
                "rule {} action count mismatch at {:?}",
                i,
                state
            );
        }
    }
}
