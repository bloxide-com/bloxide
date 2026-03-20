// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{
    capability::BloxRuntime,
    messaging::ActorId,
    spec::{MachineSpec, StateFns},
    transition::ActionResult,
    transitions,
};
use bloxide_macros::{BloxCtx, StateTopology};

use bloxide_core::lifecycle::ChildLifecycleEvent;

use crate::{
    actions::{start_children, stop_all_children, HasChildren},
    control::SupervisorControl,
    event::SupervisorEvent,
    registry::{ChildAction, ChildGroup},
};

#[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
#[handler_fns(RUNNING_FNS, SHUTTING_DOWN_FNS)]
pub enum SupervisorState {
    Running,
    ShuttingDown,
}

#[derive(BloxCtx)]
pub struct SupervisorCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasChildren<R>)]
    pub children: ChildGroup<R>,
    pub pending: ChildAction,
}

impl<R: BloxRuntime> SupervisorCtx<R> {
    pub fn all_children_stopped(&self) -> bool {
        self.children.all_stopped()
    }
}

pub struct SupervisorSpec<R: BloxRuntime>(core::marker::PhantomData<R>);

// ── Action wrappers ──────────────────────────────────────────────────────────

fn handle_done_or_failed_action<R: BloxRuntime>(
    ctx: &mut SupervisorCtx<R>,
    ev: &SupervisorEvent<R>,
) -> ActionResult {
    if let SupervisorEvent::Child(
        ChildLifecycleEvent::Done { child_id } | ChildLifecycleEvent::Failed { child_id },
    ) = ev
    {
        ctx.pending = ctx.children.handle_done_or_failed(*child_id, ctx.self_id);
    }
    ActionResult::Ok
}

fn handle_reset_action<R: BloxRuntime>(
    ctx: &mut SupervisorCtx<R>,
    ev: &SupervisorEvent<R>,
) -> ActionResult {
    if let SupervisorEvent::Child(ChildLifecycleEvent::Reset { child_id }) = ev {
        ctx.children.handle_reset(*child_id, ctx.self_id);
    }
    ActionResult::Ok
}

fn record_stopped_action<R: BloxRuntime>(
    ctx: &mut SupervisorCtx<R>,
    ev: &SupervisorEvent<R>,
) -> ActionResult {
    if let SupervisorEvent::Child(ChildLifecycleEvent::Stopped { child_id }) = ev {
        ctx.children.record_stopped(*child_id);
    }
    ActionResult::Ok
}

fn record_started_action<R: BloxRuntime>(
    ctx: &mut SupervisorCtx<R>,
    ev: &SupervisorEvent<R>,
) -> ActionResult {
    if let SupervisorEvent::Child(ChildLifecycleEvent::Started { child_id }) = ev {
        ctx.children.handle_started(*child_id);
    }
    ActionResult::Ok
}

fn record_alive_action<R: BloxRuntime>(
    ctx: &mut SupervisorCtx<R>,
    ev: &SupervisorEvent<R>,
) -> ActionResult {
    if let SupervisorEvent::Child(ChildLifecycleEvent::Alive { child_id }) = ev {
        ctx.children.handle_alive(*child_id);
    }
    ActionResult::Ok
}

fn register_child_action<R: BloxRuntime>(
    ctx: &mut SupervisorCtx<R>,
    ev: &SupervisorEvent<R>,
) -> ActionResult {
    if let SupervisorEvent::Control(SupervisorControl::RegisterChild(child)) = ev {
        ctx.children
            .add(child.id, child.lifecycle_ref.clone(), child.policy);
        ctx.children.start_child(child.id, ctx.self_id);
    }
    ActionResult::Ok
}

fn handle_health_check_action<R: BloxRuntime>(
    ctx: &mut SupervisorCtx<R>,
    ev: &SupervisorEvent<R>,
) -> ActionResult {
    if let SupervisorEvent::Control(SupervisorControl::HealthCheckTick) = ev {
        ctx.pending = ctx.children.health_check_tick(ctx.self_id);
    }
    ActionResult::Ok
}

// ── Handler tables ───────────────────────────────────────────────────────────

impl<R: BloxRuntime> SupervisorSpec<R> {
    const RUNNING_FNS: StateFns<Self> = StateFns {
        on_entry: &[start_children::<R, SupervisorCtx<R>>],
        on_exit: &[],
        transitions: transitions![
            SupervisorEvent::<R>::Child(ChildLifecycleEvent::Done { .. }) => {
                actions [handle_done_or_failed_action::<R>]
                guard(ctx, _results) {
                    ctx.pending == ChildAction::BeginShutdown => SupervisorState::ShuttingDown,
                    _ => stay,
                }
            },
            SupervisorEvent::<R>::Child(ChildLifecycleEvent::Failed { .. }) => {
                actions [handle_done_or_failed_action::<R>]
                guard(ctx, _results) {
                    ctx.pending == ChildAction::BeginShutdown => SupervisorState::ShuttingDown,
                    _ => stay,
                }
            },
            SupervisorEvent::<R>::Child(ChildLifecycleEvent::Reset { .. }) => {
                actions [handle_reset_action::<R>]
                guard(_ctx, _results) {
                    _ => stay,
                }
            },
            SupervisorEvent::<R>::Child(ChildLifecycleEvent::Started { .. }) => {
                actions [record_started_action::<R>]
                stay
            },
            SupervisorEvent::<R>::Child(ChildLifecycleEvent::Alive { .. }) => {
                actions [record_alive_action::<R>]
                stay
            },
            SupervisorEvent::<R>::Control(SupervisorControl::RegisterChild(_)) => {
                actions [register_child_action::<R>]
                stay
            },
            SupervisorEvent::<R>::Control(SupervisorControl::HealthCheckTick) => {
                actions [handle_health_check_action::<R>]
                guard(ctx, _results) {
                    ctx.pending == ChildAction::BeginShutdown => SupervisorState::ShuttingDown,
                    _ => stay,
                }
            },
            SupervisorEvent::<R>::Child(_) => { stay },
            SupervisorEvent::<R>::Control(_) => { stay },
        ],
    };

    const SHUTTING_DOWN_FNS: StateFns<Self> = StateFns {
        on_entry: &[stop_all_children::<R, SupervisorCtx<R>>],
        on_exit: &[],
        transitions: transitions![
            SupervisorEvent::<R>::Child(ChildLifecycleEvent::Stopped { .. }) => {
                actions [record_stopped_action::<R>]
                guard(ctx, _results) {
                    ctx.all_children_stopped() => reset,
                    _ => stay,
                }
            },
            SupervisorEvent::<R>::Child(_) => { stay },
            SupervisorEvent::<R>::Control(_) => { stay },
        ],
    };
}

// ── MachineSpec ──────────────────────────────────────────────────────────────

impl<R: BloxRuntime> MachineSpec for SupervisorSpec<R> {
    type State = SupervisorState;
    type Event = SupervisorEvent<R>;
    type Ctx = SupervisorCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (
        Rt::Stream<ChildLifecycleEvent>,
        Rt::Stream<SupervisorControl<R>>,
    );

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = supervisor_state_handler_table!(Self);

    fn initial_state() -> SupervisorState {
        SupervisorState::Running
    }

    fn on_init_entry(ctx: &mut SupervisorCtx<R>) {
        ctx.children.clear_counters();
        ctx.pending = ChildAction::default();
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec::Vec;

    use crate::{
        control::{RegisterChild, SupervisorControl},
        event::SupervisorEvent,
        registry::{ChildAction, ChildGroup, ChildPolicy, GroupShutdown},
        supervisor::{SupervisorCtx, SupervisorSpec, SupervisorState},
    };
    use bloxide_core::lifecycle::{ChildLifecycleEvent, LifecycleCommand};
    use bloxide_core::test_utils::{TestReceiver, TestRuntime};
    use bloxide_core::{
        capability::DynamicChannelCap, engine::DispatchOutcome, engine::MachineState, StateMachine,
    };

    fn make_supervisor(
        shutdown: GroupShutdown,
        policies: &[ChildPolicy],
    ) -> (
        StateMachine<SupervisorSpec<TestRuntime>>,
        Vec<TestReceiver<LifecycleCommand>>,
    ) {
        let mut group = ChildGroup::new(shutdown);
        let mut receivers = Vec::new();
        for (i, policy) in policies.iter().enumerate() {
            let id = i + 1;
            let (actor_ref, rx) = TestRuntime::channel::<LifecycleCommand>(id, 16);
            group.add(id, actor_ref, *policy);
            receivers.push(rx);
        }
        let ctx = SupervisorCtx {
            self_id: 100,
            children: group,
            pending: ChildAction::default(),
        };
        (StateMachine::new(ctx), receivers)
    }

    fn dispatch_child_event(
        machine: &mut StateMachine<SupervisorSpec<TestRuntime>>,
        event: ChildLifecycleEvent,
    ) -> DispatchOutcome<SupervisorState> {
        machine.dispatch(SupervisorEvent::<TestRuntime>::Child(event))
    }

    fn dispatch_control_event(
        machine: &mut StateMachine<SupervisorSpec<TestRuntime>>,
        event: SupervisorControl<TestRuntime>,
    ) -> DispatchOutcome<SupervisorState> {
        machine.dispatch(SupervisorEvent::<TestRuntime>::Control(event))
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

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Reset { child_id: 1 });
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

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 2 });
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

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Started { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Alive { child_id: 1 });
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

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Started { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::HandledNoTransition);

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Alive { child_id: 1 });
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

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Stopped { child_id: 1 });
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

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Failed { child_id: 1 });
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
        let (lifecycle_ref, mut lifecycle_rx) =
            TestRuntime::channel::<LifecycleCommand>(child_id, 8);
        let register = RegisterChild::<TestRuntime> {
            id: child_id,
            lifecycle_ref,
            policy: ChildPolicy::Stop,
        };

        let outcome =
            dispatch_control_event(&mut machine, SupervisorControl::RegisterChild(register));
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
}
