use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    engine::{DispatchOutcome, StateMachine},
    mailboxes::Mailboxes,
    messaging::{ActorId, Envelope},
    spec::MachineSpec,
};
use bloxide_supervisor::{
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    registry::{ChildGroup, ChildPolicy, GroupShutdown},
    SupervisedRunLoop,
};
use core::future::poll_fn;
use core::pin::Pin;
use core::task::Poll;
use futures_core::Stream;

use crate::{TokioRuntime, TokioSender, TokioStream};

// ── SupervisedRunLoop impl ───────────────────────────────────────────────────

impl SupervisedRunLoop for TokioRuntime {
    async fn run_supervised_actor<S: MachineSpec + 'static>(
        mut machine: StateMachine<S>,
        mut domain_mailboxes: S::Mailboxes<Self>,
        mut lifecycle_stream: Self::Stream<LifecycleCommand>,
        actor_id: ActorId,
        supervisor_notify: Self::Sender<ChildLifecycleEvent>,
    ) {
        enum LoopAction<State> {
            Continue(DispatchOutcome<State>),
            Terminated,
            Stopped,
        }

        loop {
            let action = poll_fn(|cx| {
                match Pin::new(&mut lifecycle_stream).poll_next(cx) {
                    // Lifecycle channel closed — supervisor exited without sending Stop; exit cleanly.
                    Poll::Ready(None) => return Poll::Ready(LoopAction::Stopped),
                    Poll::Ready(Some(Envelope(_, cmd))) => {
                        let action = match cmd {
                            LifecycleCommand::Start => LoopAction::Continue(machine.start()),
                            LifecycleCommand::Terminate => {
                                let o = machine.reset();
                                report_outcome::<S>(&o, actor_id, &supervisor_notify);
                                LoopAction::Terminated
                            }
                            LifecycleCommand::Stop => {
                                machine.reset();
                                let _ = <TokioRuntime as BloxRuntime>::try_send_via(
                                    &supervisor_notify,
                                    Envelope(
                                        actor_id,
                                        ChildLifecycleEvent::Stopped { child_id: actor_id },
                                    ),
                                );
                                LoopAction::Stopped
                            }
                            LifecycleCommand::Ping => {
                                let _ = <TokioRuntime as BloxRuntime>::try_send_via(
                                    &supervisor_notify,
                                    Envelope(
                                        actor_id,
                                        ChildLifecycleEvent::Alive { child_id: actor_id },
                                    ),
                                );
                                LoopAction::Continue(DispatchOutcome::Stay)
                            }
                        };
                        return Poll::Ready(action);
                    }
                    // handled above
                    Poll::Pending => {}
                }

                match domain_mailboxes.poll_next(cx) {
                    Poll::Ready(event) => {
                        Poll::Ready(LoopAction::Continue(machine.dispatch(event)))
                    }
                    Poll::Pending => Poll::Pending,
                }
            })
            .await;

            match action {
                LoopAction::Continue(o) => report_outcome::<S>(&o, actor_id, &supervisor_notify),
                LoopAction::Terminated => {}
                LoopAction::Stopped => break,
            }
        }
    }
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
        DispatchOutcome::Started(s) => {
            send(ChildLifecycleEvent::Started { child_id: actor_id });
            if S::is_error(s) {
                send(ChildLifecycleEvent::Failed { child_id: actor_id });
            } else if S::is_terminal(s) {
                send(ChildLifecycleEvent::Done { child_id: actor_id });
            }
        }
        DispatchOutcome::Transition(s) => {
            if S::is_error(s) {
                send(ChildLifecycleEvent::Failed { child_id: actor_id });
            } else if S::is_terminal(s) {
                send(ChildLifecycleEvent::Done { child_id: actor_id });
            }
        }
        DispatchOutcome::Reset => {
            send(ChildLifecycleEvent::Reset { child_id: actor_id });
        }
        _ => {}
    }
}

// ── ChildGroupBuilder ─────────────────────────────────────────────────────────

/// Builds a `ChildGroup<TokioRuntime>` and creates the supervisor's notification channel.
///
/// Usage:
/// ```ignore
/// let mut builder = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
/// bloxide_tokio::spawn_child!(builder, ping_task(machine, mbox, ping_id), ChildPolicy::Stop);
/// bloxide_tokio::spawn_child!(builder, pong_task(machine, mbox, pong_id), ChildPolicy::Stop);
/// let (children, sup_notify_rx) = builder.finish();
/// ```
pub struct ChildGroupBuilder {
    group: ChildGroup<TokioRuntime>,
    notify_tx: TokioSender<ChildLifecycleEvent>,
    notify_rx: TokioStream<ChildLifecycleEvent>,
}

impl ChildGroupBuilder {
    /// Create a new builder with the given group shutdown strategy.
    /// Allocates the supervisor notification channel immediately.
    pub fn new(shutdown: GroupShutdown) -> Self {
        let id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (notify_ref, notify_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(id, 16);
        Self {
            group: ChildGroup::new(shutdown),
            notify_tx: notify_ref.sender(),
            notify_rx,
        }
    }

    /// Register a child with the given policy and return the data needed to spawn its task.
    ///
    /// Returns `(lifecycle_rx, sup_notify_tx)`:
    /// - `lifecycle_rx` is the per-child command stream for `run_supervised_actor`.
    /// - `sup_notify_tx` is a copy of the shared supervisor notification sender.
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

    /// Consume the builder and return the finished `ChildGroup` and the
    /// supervisor's notification stream.
    pub fn finish(self) -> (ChildGroup<TokioRuntime>, TokioStream<ChildLifecycleEvent>) {
        (self.group, self.notify_rx)
    }
}
