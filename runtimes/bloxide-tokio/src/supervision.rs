// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    engine::{DispatchOutcome, MachineState, StateMachine},
    lifecycle::LifecycleCommand,
    mailboxes::Mailboxes,
    messaging::{ActorId, ActorRef, Envelope},
    spec::MachineSpec,
};

use bloxide_supervisor::{
    control::RegisterChild,
    control::SupervisorControl,
    registry::{ChildGroup, ChildPolicy, GroupShutdown},
};
use core::future::{poll_fn, Future};
use core::pin::Pin;
use core::task::Poll;
use futures_core::Stream;

use crate::{TokioRuntime, TokioSender, TokioStream};

// ── Standalone supervised actor runner ───────────────────────────────────────

/// Run a supervised actor on Tokio.
///
/// Polls lifecycle and domain mailboxes, dispatches events through the machine,
/// and reports outcomes to the supervisor.
pub async fn run_supervised_actor<S: MachineSpec + 'static>(
    mut machine: StateMachine<S>,
    mut domain_mailboxes: S::Mailboxes<TokioRuntime>,
    mut lifecycle_stream: TokioStream<LifecycleCommand>,
    actor_id: ActorId,
    supervisor_notify: TokioSender<ChildLifecycleEvent>,
) {
    enum LoopAction {
        Continue,
        Stop,
    }

    loop {
        let action = poll_fn(|cx| {
            // First check lifecycle stream (higher priority)
            match Pin::new(&mut lifecycle_stream).poll_next(cx) {
                Poll::Ready(None) => return Poll::Ready(LoopAction::Stop),
                Poll::Ready(Some(Envelope(_, cmd))) => {
                    let outcome = handle_lifecycle_direct(&mut machine, cmd);
                    report_outcome::<S>(&outcome, actor_id, &supervisor_notify);

                    return match outcome {
                        DispatchOutcome::Stopped => Poll::Ready(LoopAction::Stop),
                        _ => Poll::Ready(LoopAction::Continue),
                    };
                }
                Poll::Pending => {}
            }

            // Then check domain mailboxes
            match domain_mailboxes.poll_next(cx) {
                Poll::Ready(event) => {
                    let outcome = machine.dispatch(event);
                    report_outcome::<S>(&outcome, actor_id, &supervisor_notify);
                    Poll::Ready(LoopAction::Continue)
                }
                Poll::Pending => Poll::Pending,
            }
        })
        .await;

        match action {
            LoopAction::Continue => {}
            LoopAction::Stop => break,
        }
    }
}

/// Handle lifecycle command by delegating to engine's lifecycle handler.
///
/// This ensures state transitions fire their `on_entry`/`on_exit` callbacks.
pub fn handle_lifecycle_direct<S: MachineSpec>(
    machine: &mut StateMachine<S>,
    cmd: LifecycleCommand,
) -> DispatchOutcome<S::State> {
    machine.handle_lifecycle(cmd)
}

fn report_outcome<S: MachineSpec>(
    outcome: &DispatchOutcome<S::State>,
    actor_id: ActorId,
    notify: &TokioSender<ChildLifecycleEvent>,
) {
    let send = |event| {
        let _ = <TokioRuntime as BloxRuntime>::try_send_via(notify, Envelope(actor_id, event));
    };

    match outcome {
        DispatchOutcome::Started(MachineState::State(s)) => {
            if S::is_error(s) {
                send(ChildLifecycleEvent::Failed { child_id: actor_id });
            } else if S::is_terminal(s) {
                send(ChildLifecycleEvent::Done { child_id: actor_id });
            } else {
                send(ChildLifecycleEvent::Started { child_id: actor_id });
            }
        }
        DispatchOutcome::Transition(MachineState::State(s)) => {
            if S::is_error(s) {
                send(ChildLifecycleEvent::Failed { child_id: actor_id });
            } else if S::is_terminal(s) {
                send(ChildLifecycleEvent::Done { child_id: actor_id });
            }
        }
        DispatchOutcome::Done(MachineState::State(_)) => {
            send(ChildLifecycleEvent::Done { child_id: actor_id });
        }
        DispatchOutcome::Failed => {
            send(ChildLifecycleEvent::Failed { child_id: actor_id });
        }
        DispatchOutcome::Reset => {
            send(ChildLifecycleEvent::Reset { child_id: actor_id });
        }
        DispatchOutcome::Stopped => {
            send(ChildLifecycleEvent::Stopped { child_id: actor_id });
        }
        DispatchOutcome::Alive => {
            send(ChildLifecycleEvent::Alive { child_id: actor_id });
        }
        _ => {}
    }
}

// ── ChildGroupBuilder ─────────────────────────────────────────────────────────

pub struct ChildGroupBuilder {
    group: ChildGroup<TokioRuntime>,
    notify_tx: TokioSender<ChildLifecycleEvent>,
    notify_rx: TokioStream<ChildLifecycleEvent>,
    control_ref: ActorRef<SupervisorControl<TokioRuntime>, TokioRuntime>,
    control_rx: TokioStream<SupervisorControl<TokioRuntime>>,
    /// KillCap for policy-driven cleanup of dynamic actors.
    kill_cap: std::sync::Arc<crate::TokioKillCap>,
}

impl ChildGroupBuilder {
    pub fn new(shutdown: GroupShutdown) -> Self {
        let notify_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (notify_ref, notify_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(notify_id, 16);
        let control_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (control_ref, control_rx) = <TokioRuntime as DynamicChannelCap>::channel::<
            SupervisorControl<TokioRuntime>,
        >(control_id, 16);
        let kill_cap = std::sync::Arc::new(crate::TokioKillCap::new());
        Self {
            group: ChildGroup::with_kill_cap(shutdown, kill_cap.clone()),
            notify_tx: notify_ref.sender(),
            notify_rx,
            control_ref,
            control_rx,
            kill_cap,
        }
    }

    /// Create a new ChildGroupBuilder with a custom KillCap for dynamic actor cleanup.
    pub fn with_kill_cap(
        shutdown: GroupShutdown,
        kill_cap: std::sync::Arc<crate::TokioKillCap>,
    ) -> Self {
        let notify_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (notify_ref, notify_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(notify_id, 16);
        let control_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (control_ref, control_rx) = <TokioRuntime as DynamicChannelCap>::channel::<
            SupervisorControl<TokioRuntime>,
        >(control_id, 16);
        Self {
            group: ChildGroup::with_kill_cap(shutdown, kill_cap.clone()),
            notify_tx: notify_ref.sender(),
            notify_rx,
            control_ref,
            control_rx,
            kill_cap,
        }
    }

    pub fn add_child(
        &mut self,
        id: ActorId,
        policy: ChildPolicy,
    ) -> (
        TokioStream<LifecycleCommand>,
        TokioSender<ChildLifecycleEvent>,
    ) {
        let (lifecycle_ref, cmd_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(id, 4);
        self.group.add(id, lifecycle_ref, policy);
        (cmd_rx, self.notify_tx.clone())
    }

    /// Returns a reference to the KillCap for registering spawned tasks.
    pub fn kill_cap(&self) -> &std::sync::Arc<crate::TokioKillCap> {
        &self.kill_cap
    }

    pub fn control_ref(&self) -> ActorRef<SupervisorControl<TokioRuntime>, TokioRuntime> {
        self.control_ref.clone()
    }

    pub fn notify_sender(&self) -> TokioSender<ChildLifecycleEvent> {
        self.notify_tx.clone()
    }

    pub fn finish(
        self,
    ) -> (
        ChildGroup<TokioRuntime>,
        TokioStream<ChildLifecycleEvent>,
        TokioStream<SupervisorControl<TokioRuntime>>,
    ) {
        (self.group, self.notify_rx, self.control_rx)
    }
}

pub fn spawn_dynamic_supervised_child<F, Fut>(
    from: ActorId,
    control_ref: &ActorRef<SupervisorControl<TokioRuntime>, TokioRuntime>,
    notify_sender: &TokioSender<ChildLifecycleEvent>,
    child_id: ActorId,
    policy: ChildPolicy,
    task_builder: F,
) -> Result<(), <TokioRuntime as BloxRuntime>::TrySendError>
where
    F: FnOnce(TokioStream<LifecycleCommand>, TokioSender<ChildLifecycleEvent>, ActorId) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (lifecycle_ref, lifecycle_rx) =
        <TokioRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(child_id, 4);

    control_ref.try_send(
        from,
        SupervisorControl::RegisterChild(RegisterChild {
            id: child_id,
            lifecycle_ref,
            policy,
        }),
    )?;

    let notify = notify_sender.clone();
    tokio::spawn(task_builder(lifecycle_rx, notify, child_id));
    Ok(())
}
