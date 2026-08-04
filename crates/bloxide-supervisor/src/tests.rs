// Copyright 2025 Bloxide, all rights reserved
//! Tests for the generated supervisor state machine.
//!
//! These tests were moved from the old hand-written `supervisor.rs` and
//! adapted to use the generated `SupervisorCtx` (which has a `child_notify`
//! field and a `new()` constructor).

extern crate alloc;
use alloc::vec;
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
        group.try_add(id, actor_ref, *policy).unwrap();
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
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset { max: 3 }]);
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
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset { max: 3 }]);
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
fn stop_policy_transitions_to_shutting_down_when_children_remain() {
    // WhenAnyDone with a still-running child: the first stop triggers the
    // transition so the supervisor can stop the rest.
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    );
}

#[test]
fn stop_of_only_child_completes_immediately() {
    // Single-child group: once the child is terminal there is nothing left
    // to stop, so the supervisor self-stops without passing through
    // ShuttingDown.
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
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

    // All children terminal → the group is fully done and the supervisor
    // self-stops immediately (nothing left for a ShuttingDown pass to stop).
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
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
    // Two children: child 1 stops → ShuttingDown (child 2 still running).
    // Late Started/Alive events for child 1 hit the catch-all and are absorbed.
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert!(matches!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    ));
    for rx in receivers.iter_mut() {
        rx.drain_payloads();
    }

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Started { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Alive { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
}

#[test]
fn stop_of_single_child_completes_immediately() {
    // When the only child stops, the group is fully terminal — the guard
    // short-circuits to Decision::Stop without a ShuttingDown pass (there is
    // nobody left to stop, and waiting for an event that never comes was the
    // old wedge). The root run loop sees DispatchOutcome::Stopped and exits.
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
}

#[test]
fn failed_event_treated_same_as_done() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset { max: 3 }]);
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
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset { max: 3 }]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Tick #1: ping all monitored children (no verdict yet — no outstanding Ping).
    let outcome = dispatch_control_event(&mut machine, ChildCtrl::HealthCheckTick);
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
    let first = receivers[0].drain_payloads();
    assert_eq!(first.len(), 1);
    assert!(matches!(first[0], LifecycleCommand::Ping));

    // Tick #2 with no Alive from child: one miss (below the two-miss
    // threshold) — re-pinged, NOT yet declared rogue.
    let outcome = dispatch_control_event(&mut machine, ChildCtrl::HealthCheckTick);
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
    let second = receivers[0].drain_payloads();
    assert_eq!(second.len(), 1);
    assert!(
        matches!(second[0], LifecycleCommand::Ping),
        "one miss must not convict — expected a re-Ping, got {:?}",
        second
    );

    // Tick #3, still no Alive: second consecutive miss → rogue → child
    // policy fires (Reset), and the now-ResetPending child is re-pinged.
    let outcome = dispatch_control_event(&mut machine, ChildCtrl::HealthCheckTick);
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
    let third = receivers[0].drain_payloads();
    assert!(
        third.iter().any(|c| matches!(c, LifecycleCommand::Reset)),
        "two consecutive misses must apply the child policy, got {:?}",
        third
    );
}

// ──────────────────────────────────────────────────────────────
// Abort lifecycle tests
// ──────────────────────────────────────────────────────────────

#[test]
fn aborted_child_marked_aborted() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Reset { max: 3 }, ChildPolicy::Reset { max: 3 }],
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
        &[ChildPolicy::Reset { max: 3 }, ChildPolicy::Reset { max: 3 }],
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
fn aborted_child_participates_in_shutdown_check() {
    // Externally-originated Aborted events count toward group shutdown, like
    // every other terminal signal. (This reverses the old "Aborted does not
    // re-evaluate shutdown" behavior, which wedged WhenAllDone groups whose
    // children were killed or aborted from outside the supervisor.)
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child 1 stopped, child 2 aborted — both terminal → the group is fully
    // done, so the supervisor self-stops immediately (no ShuttingDown
    // pass-through: there is nothing left to stop).
    dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Aborted { child_id: 2 });
    assert_eq!(
        outcome,
        DispatchOutcome::Stopped,
        "Aborted on the last non-terminal child must complete the group shutdown"
    );
}

#[test]
fn killed_child_participates_in_shutdown_check() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Killed { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
}

#[test]
fn unknown_done_does_not_trigger_when_any_done_shutdown() {
    // A Done from an unregistered child must never trigger group shutdown —
    // WhenAnyDone would otherwise fire on a forged or miswired report.
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 999 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
    assert!(matches!(
        machine.current_state(),
        MachineState::State(SupervisorState::Running)
    ));
}

#[test]
fn duplicate_registration_is_absorbed() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Register child 77 (new) — accepted and started.
    let (lifecycle_ref, mut lifecycle_rx) = TestRuntime::channel::<LifecycleCommand>(77, 8);
    let register = RegisterChild::<TestRuntime> {
        id: 77,
        lifecycle_ref,
        policy: ChildPolicy::Stop,
    };
    dispatch_control_event(&mut machine, ChildCtrl::RegisterChild(register));
    assert_eq!(lifecycle_rx.drain_payloads().len(), 1, "first Start");

    // Register child 77 again — rejected as duplicate: no second Start.
    let (dup_ref, mut dup_rx) = TestRuntime::channel::<LifecycleCommand>(77, 8);
    let dup = RegisterChild::<TestRuntime> {
        id: 77,
        lifecycle_ref: dup_ref,
        policy: ChildPolicy::Stop,
    };
    let outcome = dispatch_control_event(&mut machine, ChildCtrl::RegisterChild(dup));
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
    assert!(
        dup_rx.drain_payloads().is_empty(),
        "a duplicate registration must not start a phantom entry"
    );
    // And the original entry sees no second Start either.
    assert!(lifecycle_rx.drain_payloads().is_empty());
}

// ──────────────────────────────────────────────────────────────
// Dynamic child registration tests
// ──────────────────────────────────────────────────────────────

#[test]
fn register_dynamic_child_adds_and_starts() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAllDone, &[ChildPolicy::Reset { max: 3 }]);
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
        kill_handle: 0,
        policy: ChildPolicy::Reset { max: 3 },
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
    // WhenAnyDone + ChildPolicy::Reset { max: 3 }, so the supervisor never actually left
    // Running.)
    // Two children: child 1 stops with Stop policy → WhenAnyDone →
    // ShuttingDown (child 2 still running), so the assertion below exercises
    // a genuine ShuttingDown state.
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

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
        kill_handle: 0,
        policy: ChildPolicy::Reset { max: 3 },
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
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Reset { max: 3 }]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Child reports Done (clean completion). Deregistration empties the
    // group — fully terminal — so the supervisor self-stops immediately
    // rather than passing through an event-less ShuttingDown.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::Stopped);

    // No Reset is sent — Done deregisters, it is not a fault.
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

    // Second Done: last child deregistered → group empty → immediate stop.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
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

// ──────────────────────────────────────────────────────────────
// ShuttingDown liveness (record-all, wait-forever)
//
// Every terminal signal completes the shutdown: Stopped, Done, Failed
// (parked-error, task alive), Aborted, Killed, and Gone (a Closed channel
// observed while flushing pending commands). No timeouts by design.
// ──────────────────────────────────────────────────────────────

/// Two children in WhenAnyDone: child 1 (Stop) is already stopped, so the
/// supervisor is in ShuttingDown; child 2 (Reset) is still being stopped.
fn make_shutting_down_machine() -> (StateMachine<Spec>, Vec<TestReceiver<LifecycleCommand>>) {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Stop, ChildPolicy::Reset { max: 3 }],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert!(matches!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    ));
    // Drain the Stop sent to child 2 by stop_all_children.
    for rx in receivers.iter_mut() {
        rx.drain_payloads();
    }
    (machine, receivers)
}

#[test]
fn shutting_down_completes_on_failed() {
    // A child that fails while the supervisor is shutting down is recorded as
    // terminal (parked-error, task alive) — the old topology dropped the
    // event on the catch-all and wedged.
    let (mut machine, _receivers) = make_shutting_down_machine();

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Failed { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
}

#[test]
fn shutting_down_completes_on_aborted() {
    let (mut machine, _receivers) = make_shutting_down_machine();

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Aborted { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
}

#[test]
fn shutting_down_completes_on_killed() {
    let (mut machine, _receivers) = make_shutting_down_machine();

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Killed { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
}

/// Two children, WhenAnyDone: child 1 (Stop policy) on a normal channel,
/// child 2 (Stop policy) on a capacity-1 lifecycle channel so stop_all's Stop
/// pends behind the undelivered Start.
///
/// After Start and Stopped{1}, the supervisor is in ShuttingDown with
/// child 2's Stop queued as pending. Returns the machine, both child
/// receivers (kept alive — dropping one closes the channel), and child 2's id.
fn make_shutting_down_with_pending_stop(
) -> (StateMachine<Spec>, Vec<TestReceiver<LifecycleCommand>>) {
    let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let (lc1, rx1) = TestRuntime::channel::<LifecycleCommand>(1, 16);
    let (lc2, rx2) = TestRuntime::channel::<LifecycleCommand>(2, 1);
    group.try_add(1, lc1, ChildPolicy::Stop).unwrap();
    group.try_add(2, lc2, ChildPolicy::Stop).unwrap();
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(100, 16);
    let ctx = SupervisorCtx::new(100, group, notify_ref);
    let mut machine = StateMachine::new(ctx);
    let mut receivers = vec![rx1, rx2];

    // Start fills child 2's capacity-1 channel ([Start]).
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));

    // Child 1 reports Stopped → WhenAnyDone → ShuttingDown. stop_all delivers
    // Stop to child 1 (already stopped — the engine acks Stop-in-Init) but
    // cannot deliver to child 2 (channel full) → child 2's Stop is pending.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert!(matches!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    ));
    receivers[0].drain_payloads(); // child 1: Start + Stop
    let child2_queued = receivers[1].drain_payloads();
    assert!(
        matches!(child2_queued[..], [LifecycleCommand::Start]),
        "child 2 must hold only the Start — its Stop is pending, got {:?}",
        child2_queued
    );
    (machine, receivers)
}

#[test]
fn shutting_down_tick_flushes_pending_stop() {
    // A Stop lost to a full channel is retried by the flush wired into every
    // transition; once delivered, the child's Stopped ack completes shutdown.
    let (mut machine, mut receivers) = make_shutting_down_with_pending_stop();

    // HealthCheckTick flushes the pending Stop to child 2 (channel drained).
    let outcome = dispatch_control_event(&mut machine, ChildCtrl::HealthCheckTick);
    assert_eq!(
        outcome,
        DispatchOutcome::HandledNoTransition,
        "child 2 is not terminal yet — shutdown must wait for its report"
    );
    let cmds = receivers[1].drain_payloads();
    assert!(
        matches!(cmds[..], [LifecycleCommand::Stop]),
        "the pending Stop must be delivered by the flush, got {:?}",
        cmds
    );

    // Child 2's Stopped ack (the engine acks Stop-in-Init) completes it.
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::Stopped);
}

#[test]
fn shutting_down_completes_when_pending_stop_target_dies() {
    // The Stop is pending when child 2's task dies (its lifecycle channel
    // closes). The next event pass observes Closed → Gone → terminal →
    // shutdown completes. This is the confirm-before-record replacement for
    // the old wedge (lost Stop + dead child = supervisor stuck forever).
    let (mut machine, receivers) = make_shutting_down_with_pending_stop();
    drop(receivers); // both child tasks die — channels close

    let outcome = dispatch_control_event(&mut machine, ChildCtrl::HealthCheckTick);
    assert_eq!(
        outcome,
        DispatchOutcome::Stopped,
        "Closed channel while flushing must mark the child Gone and complete shutdown"
    );
}
