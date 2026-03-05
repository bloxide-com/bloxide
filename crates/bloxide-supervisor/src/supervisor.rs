// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{
    capability::BloxRuntime,
    messaging::ActorId,
    spec::{MachineSpec, StateFns},
    transition::ActionResult,
    transitions,
};
use bloxide_macros::{BloxCtx, StateTopology};

use crate::{
    actions::{start_children, stop_all_children, HasChildren},
    event::SupervisorEvent,
    lifecycle::ChildLifecycleEvent,
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
    ev: &SupervisorEvent,
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
    ev: &SupervisorEvent,
) -> ActionResult {
    if let SupervisorEvent::Child(ChildLifecycleEvent::Reset { child_id }) = ev {
        ctx.children.handle_reset(*child_id, ctx.self_id);
    }
    ActionResult::Ok
}

fn record_stopped_action<R: BloxRuntime>(
    ctx: &mut SupervisorCtx<R>,
    ev: &SupervisorEvent,
) -> ActionResult {
    if let SupervisorEvent::Child(ChildLifecycleEvent::Stopped { child_id }) = ev {
        ctx.children.record_stopped(*child_id);
    }
    ActionResult::Ok
}

// ── Handler tables ───────────────────────────────────────────────────────────

impl<R: BloxRuntime> SupervisorSpec<R> {
    const RUNNING_FNS: StateFns<Self> = StateFns {
        on_entry: &[start_children::<R, SupervisorCtx<R>>],
        on_exit: &[],
        transitions: transitions![
            SupervisorEvent::Child(ChildLifecycleEvent::Done { .. }) => {
                actions [handle_done_or_failed_action::<R>]
                guard(ctx, _results) {
                    ctx.pending == ChildAction::BeginShutdown => SupervisorState::ShuttingDown,
                    _ => stay,
                }
            },
            SupervisorEvent::Child(ChildLifecycleEvent::Failed { .. }) => {
                actions [handle_done_or_failed_action::<R>]
                guard(ctx, _results) {
                    ctx.pending == ChildAction::BeginShutdown => SupervisorState::ShuttingDown,
                    _ => stay,
                }
            },
            SupervisorEvent::Child(ChildLifecycleEvent::Reset { .. }) => {
                actions [handle_reset_action::<R>]
                guard(_ctx, _results) {
                    _ => stay,
                }
            },
            SupervisorEvent::Child(_) => { stay },
        ],
    };

    const SHUTTING_DOWN_FNS: StateFns<Self> = StateFns {
        on_entry: &[stop_all_children::<R, SupervisorCtx<R>>],
        on_exit: &[],
        transitions: transitions![
            SupervisorEvent::Child(ChildLifecycleEvent::Stopped { .. }) => {
                actions [record_stopped_action::<R>]
                guard(ctx, _results) {
                    ctx.all_children_stopped() => reset,
                    _ => stay,
                }
            },
            SupervisorEvent::Child(_) => { stay },
        ],
    };
}

// ── MachineSpec ──────────────────────────────────────────────────────────────

impl<R: BloxRuntime> MachineSpec for SupervisorSpec<R> {
    type State = SupervisorState;
    type Event = SupervisorEvent;
    type Ctx = SupervisorCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<ChildLifecycleEvent>,);

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
        event::SupervisorEvent,
        lifecycle::{ChildLifecycleEvent, LifecycleCommand},
        registry::{ChildAction, ChildGroup, ChildPolicy, GroupShutdown},
        supervisor::{SupervisorCtx, SupervisorSpec, SupervisorState},
    };
    use bloxide_core::test_utils::{TestReceiver, TestRuntime};
    use bloxide_core::{capability::DynamicChannelCap, engine::DispatchOutcome, StateMachine};

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
        machine.dispatch(SupervisorEvent::Child(event))
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
        let outcome = machine.start();
        assert_eq!(outcome, DispatchOutcome::Started(SupervisorState::Running));

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
        machine.start();
        drain_start_commands(&mut receivers);

        let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::Stay);

        let cmds = receivers[0].drain_payloads();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], LifecycleCommand::Terminate));
    }

    #[test]
    fn restart_policy_reset_sends_start() {
        let (mut machine, mut receivers) = make_supervisor(
            GroupShutdown::WhenAnyDone,
            &[ChildPolicy::Restart { max: 3 }],
        );
        machine.start();
        drain_start_commands(&mut receivers);

        dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
        receivers[0].drain_payloads();

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Reset { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::Stay);

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
        machine.start();
        drain_start_commands(&mut receivers);

        dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
        receivers[0].drain_payloads();
        dispatch_child_event(&mut machine, ChildLifecycleEvent::Reset { child_id: 1 });
        receivers[0].drain_payloads();

        let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
        assert_eq!(
            outcome,
            DispatchOutcome::Transition(SupervisorState::ShuttingDown)
        );
    }

    #[test]
    fn stop_policy_transitions_to_shutting_down() {
        let (mut machine, mut receivers) =
            make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
        machine.start();
        drain_start_commands(&mut receivers);

        let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
        assert_eq!(
            outcome,
            DispatchOutcome::Transition(SupervisorState::ShuttingDown)
        );
    }

    #[test]
    fn shutting_down_stops_all_and_resets_when_done() {
        let (mut machine, mut receivers) = make_supervisor(
            GroupShutdown::WhenAnyDone,
            &[ChildPolicy::Stop, ChildPolicy::Stop],
        );
        machine.start();
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
        assert_eq!(outcome, DispatchOutcome::Stay);

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
        machine.start();
        drain_start_commands(&mut receivers);

        let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::Stay);

        let outcome = dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 2 });
        assert_eq!(
            outcome,
            DispatchOutcome::Transition(SupervisorState::ShuttingDown)
        );
    }

    #[test]
    fn stray_events_absorbed_in_running() {
        let (mut machine, mut receivers) =
            make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
        machine.start();
        drain_start_commands(&mut receivers);

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Started { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::Stay);

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Alive { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::Stay);
    }

    #[test]
    fn stray_events_absorbed_in_shutting_down() {
        let (mut machine, mut receivers) =
            make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
        machine.start();
        drain_start_commands(&mut receivers);

        dispatch_child_event(&mut machine, ChildLifecycleEvent::Done { child_id: 1 });
        for rx in receivers.iter_mut() {
            rx.drain_payloads();
        }

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Started { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::Stay);

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Alive { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::Stay);
    }

    #[test]
    fn on_init_entry_clears_counters() {
        let (mut machine, mut receivers) =
            make_supervisor(GroupShutdown::WhenAnyDone, &[ChildPolicy::Stop]);
        machine.start();
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
        machine.start();
        drain_start_commands(&mut receivers);

        let outcome =
            dispatch_child_event(&mut machine, ChildLifecycleEvent::Failed { child_id: 1 });
        assert_eq!(outcome, DispatchOutcome::Stay);

        let cmds = receivers[0].drain_payloads();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], LifecycleCommand::Terminate));
    }
}
