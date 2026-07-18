// Copyright 2025 Bloxide, all rights reserved
//! Tests for the generated supervisor state machine.
//!
//! These tests were moved from the old hand-written `supervisor.rs` and
//! adapted to use the generated `SupervisorCtx` (which has a `child_notify`
//! field and a `new()` constructor).

extern crate alloc;
use alloc::vec::Vec;

use crate::{
    control::{RegisterChild, SupervisorControl},
    event::SupervisorEvent,
    registry::{ChildAction, ChildGroup, ChildPolicy, GroupShutdown, RestartStrategy},
    SupervisorCtx, SupervisorSpec, SupervisorState,
};
use bloxide_core::lifecycle::{ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::test_utils::{TestReceiver, TestRuntime};
use bloxide_core::{
    capability::DynamicChannelCap, engine::DispatchOutcome, engine::MachineState, StateMachine,
};

// The supervisor's generic arity depends on the `dynamic` feature:
//   - Without dynamic: SupervisorSpec<R>, SupervisorCtx::new(group, id, notify)
//   - With dynamic:    SupervisorSpec<R, F>, SupervisorCtx::new(group, id, notify, factory)
// These aliases and the helper functions below abstract over both shapes so
// the test bodies remain identical regardless of feature flags.
#[cfg(not(feature = "dynamic"))]
type Spec = SupervisorSpec<TestRuntime>;
#[cfg(feature = "dynamic")]
type Spec = SupervisorSpec<TestRuntime, crate::NoSpawnFactory>;

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
    #[cfg(not(feature = "dynamic"))]
    let ctx = SupervisorCtx::new(group, 100, notify_ref);
    #[cfg(feature = "dynamic")]
    let ctx = SupervisorCtx::new(group, 100, notify_ref, crate::NoSpawnFactory);
    (StateMachine::new(ctx), receivers)
}

fn make_supervisor_with_strategy(
    shutdown: GroupShutdown,
    policies: &[ChildPolicy],
    strategy: RestartStrategy,
) -> (StateMachine<Spec>, Vec<TestReceiver<LifecycleCommand>>) {
    let mut group = ChildGroup::new(shutdown).with_restart_strategy(strategy);
    let mut receivers = Vec::new();
    for (i, policy) in policies.iter().enumerate() {
        let id = i + 1;
        let (actor_ref, rx) = TestRuntime::channel::<LifecycleCommand>(id, 16);
        group.add(id, actor_ref, *policy);
        receivers.push(rx);
    }
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(100, 16);
    #[cfg(not(feature = "dynamic"))]
    let ctx = SupervisorCtx::new(group, 100, notify_ref);
    #[cfg(feature = "dynamic")]
    let ctx = SupervisorCtx::new(group, 100, notify_ref, crate::NoSpawnFactory);
    (StateMachine::new(ctx), receivers)
}

fn dispatch_child_event(
    machine: &mut StateMachine<Spec>,
    event: ChildLifecycleEvent,
) -> DispatchOutcome<SupervisorState> {
    #[cfg(not(feature = "dynamic"))]
    let ev = SupervisorEvent::<TestRuntime>::Child(event);
    #[cfg(feature = "dynamic")]
    let ev = SupervisorEvent::<TestRuntime, crate::NoSpawnFactory>::Child(event);
    machine.dispatch(ev)
}

fn dispatch_control_event(
    machine: &mut StateMachine<Spec>,
    event: SupervisorControl<TestRuntime>,
) -> DispatchOutcome<SupervisorState> {
    #[cfg(not(feature = "dynamic"))]
    let ev = SupervisorEvent::<TestRuntime>::Control(event);
    #[cfg(feature = "dynamic")]
    let ev = SupervisorEvent::<TestRuntime, crate::NoSpawnFactory>::Control(event);
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
        &[ChildPolicy::Restart { max: 3 }],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let cmds = receivers[0].drain_payloads();
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds[0], LifecycleCommand::Reset));
}

#[test]
fn restart_policy_reset_sends_start() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Restart { max: 3 }],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    receivers[0].drain_payloads();

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Reset { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let cmds = receivers[0].drain_payloads();
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds[0], LifecycleCommand::Start));
}

#[test]
fn restart_limit_exhausted_shuts_down() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Restart { max: 1 }],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    receivers[0].drain_payloads();
    dispatch_child_event(&mut machine, ChildLifecycleEvent::Reset { child_id: 1 });
    receivers[0].drain_payloads();

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    assert_eq!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    );
}

#[test]
fn stop_policy_transitions_to_shutting_down() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    assert_eq!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    );
}

#[test]
fn shutting_down_stops_all_and_resets_when_done() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });

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

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::Reset);
}

#[test]
fn when_all_done_waits_for_all_children() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAllDone,
        &[ChildPolicy::Stop, ChildPolicy::Stop],
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 2 });
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

    dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    for rx in receivers.iter_mut() {
        rx.drain_payloads();
    }

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Started { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Alive { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
}

#[test]
fn on_init_entry_clears_counters() {
    let (mut machine, mut receivers) =
        make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    for rx in receivers.iter_mut() {
        rx.drain_payloads();
    }

    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
    assert_eq!(outcome, DispatchOutcome::Reset);

    assert_eq!(machine.ctx().pending, ChildAction::default());
    assert!(!machine.ctx().children.all_stopped());
}

#[test]
fn failed_event_treated_same_as_done() {
    let (mut machine, mut receivers) = make_supervisor(
        GroupShutdown::WhenAnyDone,
        &[ChildPolicy::Restart { max: 3 }],
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
        &[ChildPolicy::Restart { max: 2 }],
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

// ── Restart strategy tests ──────────────────────────────────────────────

#[test]
fn one_for_one_restarts_only_failed_child() {
    let (mut machine, mut receivers) = make_supervisor_with_strategy(
        GroupShutdown::WhenAnyDone,
        &[
            ChildPolicy::Restart { max: 3 },
            ChildPolicy::Restart { max: 3 },
            ChildPolicy::Restart { max: 3 },
        ],
        RestartStrategy::OneForOne,
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Fail child 2
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    // Only child 2 should receive Reset
    assert!(
        receivers[0].drain_payloads().is_empty(),
        "child 1 should not receive any command"
    );
    let cmds2 = receivers[1].drain_payloads();
    assert_eq!(cmds2.len(), 1);
    assert!(matches!(cmds2[0], LifecycleCommand::Reset));
    assert!(
        receivers[2].drain_payloads().is_empty(),
        "child 3 should not receive any command"
    );
}

#[test]
fn one_for_all_restarts_all_children() {
    let (mut machine, mut receivers) = make_supervisor_with_strategy(
        GroupShutdown::WhenAnyDone,
        &[
            ChildPolicy::Restart { max: 3 },
            ChildPolicy::Restart { max: 3 },
            ChildPolicy::Restart { max: 3 },
        ],
        RestartStrategy::OneForAll,
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Fail child 2
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    // All children should receive Reset
    for (i, rx) in receivers.iter_mut().enumerate() {
        let cmds = rx.drain_payloads();
        assert_eq!(
            cmds.len(),
            1,
            "child {} should receive exactly one Reset",
            i + 1
        );
        assert!(
            matches!(cmds[0], LifecycleCommand::Reset),
            "child {} should receive Reset, got {:?}",
            i + 1,
            cmds[0]
        );
    }
}

#[test]
fn rest_for_one_restarts_subsequent_children() {
    let (mut machine, mut receivers) = make_supervisor_with_strategy(
        GroupShutdown::WhenAnyDone,
        &[
            ChildPolicy::Restart { max: 3 },
            ChildPolicy::Restart { max: 3 },
            ChildPolicy::Restart { max: 3 },
            ChildPolicy::Restart { max: 3 },
        ],
        RestartStrategy::RestForOne,
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // Fail child 2 (index 1)
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 2 });
    assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

    // Child 1 (index 0) should NOT receive any command
    assert!(
        receivers[0].drain_payloads().is_empty(),
        "child 1 should not receive any command (it is before the failed child)"
    );

    // Children 2, 3, 4 should receive Reset
    for i in 1..=3 {
        let cmds = receivers[i].drain_payloads();
        assert_eq!(
            cmds.len(),
            1,
            "child {} should receive exactly one Reset",
            i + 1
        );
        assert!(
            matches!(cmds[0], LifecycleCommand::Reset),
            "child {} should receive Reset, got {:?}",
            i + 1,
            cmds[0]
        );
    }
}

#[test]
fn restart_strategy_respects_max_restarts() {
    let (mut machine, mut receivers) = make_supervisor_with_strategy(
        GroupShutdown::WhenAnyDone,
        [
            ChildPolicy::Restart { max: 1 },
            ChildPolicy::Restart { max: 1 },
        ]
        .as_slice(),
        RestartStrategy::OneForAll,
    );
    machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
    drain_start_commands(&mut receivers);

    // First failure of child 1: both children get Reset (restarts 0 < 1)
    dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    for rx in receivers.iter_mut() {
        let cmds = rx.drain_payloads();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], LifecycleCommand::Reset));
    }

    // Both children report Reset: each gets Start, restart counter increments to 1
    dispatch_child_event(&mut machine, ChildLifecycleEvent::Reset { child_id: 1 });
    dispatch_child_event(&mut machine, ChildLifecycleEvent::Reset { child_id: 2 });
    for rx in receivers.iter_mut() {
        let cmds = rx.drain_payloads();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], LifecycleCommand::Start));
    }

    // Second failure of child 1: restarts 1 >= max 1 → permanently done → shutdown
    let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
    assert_eq!(
        outcome,
        DispatchOutcome::Transition(MachineState::State(SupervisorState::ShuttingDown))
    );
}
