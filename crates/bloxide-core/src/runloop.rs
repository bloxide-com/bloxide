// Copyright 2025 Bloxide, all rights reserved
//! The unified actor run loop.
//!
//! A single generic [`run`] function that handles all actor execution modes:
//! root, supervised, supervised-with-abort, unsupervised, and bare (test).
//! Behaviour is controlled entirely by [`RunConfig`].

use core::future::poll_fn;
use core::pin::Pin;
use core::task::Poll;

use futures_core::Stream;

use crate::capability::BloxRuntime;
use crate::engine::{DispatchOutcome, StateMachine};
use crate::lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand};
use crate::mailboxes::Mailboxes;
use crate::messaging::{ActorId, Envelope};
use crate::spec::MachineSpec;
use crate::supervision::report_outcome;

// ── RunConfig ───────────────────────────────────────────────────────────────

/// Configuration for the unified [`run`] loop.
///
/// All fields are optional — the loop adapts to what is provided:
///
/// | Field | Effect when `Some` | Effect when `None` |
/// |-------|-------------------|--------------------|
/// | `lifecycle` | Polls lifecycle commands (Start/Stop/Reset) | No lifecycle stream |
/// | `abort` | Polls abort commands (cooperative self-termination) | No abort stream |
/// | `supervisor_notify` | Reports outcomes to supervisor | No outcome reporting |
/// | `auto_start` | Calls `handle_lifecycle(Start)` before entering the loop | Actor starts via lifecycle stream |
/// | `exit_on_stop` | `DispatchOutcome::Stopped` breaks the loop | Actor stays alive in Init, waiting for Start/Reset |
/// | `exit_on_fail` | `DispatchOutcome::Failed` breaks the loop | Actor stays alive in its absorbing error state; supervisor's `ChildPolicy` decides (Reset revives) |
///
/// **Typical configurations:**
///
/// - **Root supervisor**: `exit_on_stop = true`, `exit_on_fail = true`, no lifecycle/abort/notify
/// - **Supervised child**: `lifecycle + supervisor_notify`, `exit_on_stop = false`, `exit_on_fail = false`
/// - **Supervised child with kill capability**: `lifecycle + abort + supervisor_notify`, `exit_on_stop = false`, `exit_on_fail = false`
/// - **Unsupervised actor**: `auto_start = true`, `exit_on_stop = true`, `exit_on_fail = true`
/// - **Bare (test)**: no lifecycle/abort/notify, `auto_start = false`, `exit_on_stop = true`, `exit_on_fail = true`
pub struct RunConfig<R: BloxRuntime> {
    /// Lifecycle command stream from the supervisor. `None` for unsupervised/root actors.
    pub lifecycle: Option<R::Stream<LifecycleCommand>>,
    /// Abort command stream for cooperative kill. `None` for static (NoKill) children.
    pub abort: Option<R::Stream<AbortCommand>>,
    /// Sender to report `ChildLifecycleEvent` to the supervisor. `None` for unsupervised/root.
    pub supervisor_notify: Option<R::Sender<ChildLifecycleEvent>>,
    /// Auto-start the actor before entering the loop. Use for unsupervised actors.
    pub auto_start: bool,
    /// Exit the loop when `DispatchOutcome::Stopped` is observed.
    /// `true` for root/unsupervised, `false` for supervised (stays alive in Init).
    pub exit_on_stop: bool,
    /// Exit the loop when `DispatchOutcome::Failed` is observed.
    /// `true` for root/unsupervised, `false` for supervised: the actor parks in
    /// its (absorbing) error state and stays alive so the supervisor's
    /// `ChildPolicy` can reset, stop, abort, or kill it.
    pub exit_on_fail: bool,
}

impl<R: BloxRuntime> RunConfig<R> {
    /// Configuration for a root supervisor or root actor.
    ///
    /// No lifecycle stream, no abort, no supervisor notify.
    /// Exits on `Stopped` (program done signal from root supervisor).
    pub fn root() -> Self {
        Self {
            lifecycle: None,
            abort: None,
            supervisor_notify: None,
            auto_start: false,
            exit_on_stop: true,
            exit_on_fail: true,
        }
    }

    /// Configuration for a supervised child actor.
    ///
    /// Lifecycle stream + supervisor notify. Stays alive on `Stopped`.
    pub fn supervised(
        lifecycle: R::Stream<LifecycleCommand>,
        supervisor_notify: R::Sender<ChildLifecycleEvent>,
    ) -> Self {
        Self {
            lifecycle: Some(lifecycle),
            abort: None,
            supervisor_notify: Some(supervisor_notify),
            auto_start: false,
            exit_on_stop: false,
            exit_on_fail: false,
        }
    }

    /// Configuration for a supervised child with kill capability.
    ///
    /// Lifecycle + abort + supervisor notify. Stays alive on `Stopped`.
    pub fn supervised_with_abort(
        lifecycle: R::Stream<LifecycleCommand>,
        abort: R::Stream<AbortCommand>,
        supervisor_notify: R::Sender<ChildLifecycleEvent>,
    ) -> Self {
        Self {
            lifecycle: Some(lifecycle),
            abort: Some(abort),
            supervisor_notify: Some(supervisor_notify),
            auto_start: false,
            exit_on_stop: false,
            exit_on_fail: false,
        }
    }

    /// Configuration for an unsupervised actor that auto-starts and exits on stop.
    pub fn unsupervised() -> Self {
        Self {
            lifecycle: None,
            abort: None,
            supervisor_notify: None,
            auto_start: true,
            exit_on_stop: true,
            exit_on_fail: true,
        }
    }

    /// Configuration for a bare actor (no lifecycle, no supervisor, no auto-start).
    /// Used by the test runtime and bare-style callers.
    /// The actor expects lifecycle commands (including Start) to arrive via
    /// the event stream.
    pub fn bare() -> Self {
        Self {
            lifecycle: None,
            abort: None,
            supervisor_notify: None,
            auto_start: false,
            exit_on_stop: true,
            exit_on_fail: true,
        }
    }
}

// ── The single run loop ────────────────────────────────────────────────────

/// The unified run loop for all bloxide actors.
///
/// Polls lifecycle → abort → domain mailboxes in priority order, dispatches
/// events through the machine, and reports outcomes to the supervisor (if any).
/// Yields to the executor via `R::yield_now()` after each message.
///
/// The loop exits when:
/// - `exit_on_stop` is true and `DispatchOutcome::Stopped` is observed
/// - `exit_on_fail` is true and `DispatchOutcome::Failed` is observed
/// - `DispatchOutcome::Aborted` is observed (always exits)
/// - `DispatchOutcome::Done` is observed (always exits — clean self-termination)
/// - The lifecycle or abort stream returns `Poll::Ready(None)` (stream
///   closed — always fatal; shutdown flows through these streams)
/// - The domain mailboxes return `Poll::Ready(None)` — ALL domain streams
///   closed (all-streams-close, issue #134): no domain sender remains
///   anywhere, so the actor cannot be reached at all
///
/// Stream-closed exits are classified by who could consume a report:
/// - **Lifecycle / abort stream closed** — the senders live only in the
///   supervisor's child group, so closure is always supervisor-initiated
///   (deregistration or app teardown). Expected exit; nothing is reported.
/// - **All domain streams closed** (all-streams-close, issue #134) — domain
///   senders live in peers, parents, and wiring, so closure means the actor
///   became unreachable *while the supervisor is still alive*. Abnormal for
///   an operational actor: the loop reports `Failed` before ending the task,
///   and the child policy applies (its command send fails Closed — the task
///   is already gone — and the supervisor records task-gone). An actor
///   suspended in Init stays silent — a properly stopped actor losing its
///   domain senders is the expected app-teardown cascade.
///
/// When `exit_on_stop` is false (supervised actors), `Stopped` is NOT terminal —
/// the actor self-suspends to Init and the task stays alive, waiting for a
/// future `Start` or `Reset` from the supervisor. Likewise, when `exit_on_fail`
/// is false, `Failed` is NOT terminal — the actor parks in its absorbing error
/// state and the supervisor's `ChildPolicy` decides (Reset restarts it).
pub async fn run<S, M, R>(
    mut machine: StateMachine<S>,
    mut domain_mailboxes: M,
    config: RunConfig<R>,
    actor_id: ActorId,
) where
    S: MachineSpec + 'static,
    M: Mailboxes<S::Event>,
    R: BloxRuntime,
{
    // Optional auto-start for unsupervised actors
    if config.auto_start {
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
        if let Some(ref notify) = config.supervisor_notify {
            report_outcome::<S, R>(&outcome, actor_id, notify);
        }
        match &outcome {
            // The engine normalizes Started(error-state) to Failed, so the
            // exit_on_fail arm covers the error case uniformly.
            DispatchOutcome::Failed if config.exit_on_fail => return,
            DispatchOutcome::Stopped if config.exit_on_stop => return,
            _ => {}
        }
    }

    let mut lifecycle_stream = config.lifecycle;
    let mut abort_stream = config.abort;
    let supervisor_notify = config.supervisor_notify;

    enum LoopAction {
        Continue,
        Stop,
    }

    loop {
        let action = poll_fn(|cx| {
            // 1. Lifecycle stream (highest priority)
            if let Some(ref mut ls) = lifecycle_stream {
                match Pin::new(ls).poll_next(cx) {
                    // Lifecycle-stream closure is always supervisor-initiated
                    // (deregistration after Done, or app teardown dropping the
                    // group) — an expected exit, reported to nobody. The only
                    // consumer of a report here would be the supervisor, and
                    // this closure implies it is already gone.
                    Poll::Ready(None) => return Poll::Ready(LoopAction::Stop),
                    Poll::Ready(Some(Envelope(_, cmd))) => {
                        let outcome = machine.handle_lifecycle(cmd);
                        if let Some(ref notify) = supervisor_notify {
                            report_outcome::<S, R>(&outcome, actor_id, notify);
                        }
                        match &outcome {
                            DispatchOutcome::Aborted | DispatchOutcome::Done => {
                                return Poll::Ready(LoopAction::Stop);
                            }
                            DispatchOutcome::Failed if config.exit_on_fail => {
                                return Poll::Ready(LoopAction::Stop);
                            }
                            DispatchOutcome::Stopped if config.exit_on_stop => {
                                return Poll::Ready(LoopAction::Stop);
                            }
                            _ => return Poll::Ready(LoopAction::Continue),
                        }
                    }
                    Poll::Pending => {}
                }
            }

            // 2. Abort stream (high priority — serviced before domain messages)
            if let Some(ref mut as_) = abort_stream {
                match Pin::new(as_).poll_next(cx) {
                    // Same as lifecycle-stream closure: the abort sender is
                    // held only by the supervisor's group, so closure is
                    // supervisor-initiated teardown — expected, unreported.
                    Poll::Ready(None) => return Poll::Ready(LoopAction::Stop),
                    Poll::Ready(Some(Envelope(_, AbortCommand::Abort { .. }))) => {
                        if let Some(ref notify) = supervisor_notify {
                            report_outcome::<S, R>(&DispatchOutcome::Aborted, actor_id, notify);
                        }
                        return Poll::Ready(LoopAction::Stop);
                    }
                    Poll::Pending => {}
                }
            }

            // 3. Domain mailboxes
            match domain_mailboxes.poll_next(cx) {
                Poll::Ready(Some(event)) => {
                    let outcome = machine.dispatch(event);
                    if let Some(ref notify) = supervisor_notify {
                        report_outcome::<S, R>(&outcome, actor_id, notify);
                    }
                    match &outcome {
                        DispatchOutcome::Aborted | DispatchOutcome::Done => {
                            Poll::Ready(LoopAction::Stop)
                        }
                        DispatchOutcome::Failed if config.exit_on_fail => {
                            Poll::Ready(LoopAction::Stop)
                        }
                        DispatchOutcome::Stopped if config.exit_on_stop => {
                            Poll::Ready(LoopAction::Stop)
                        }
                        _ => Poll::Ready(LoopAction::Continue),
                    }
                }
                Poll::Ready(None) => {
                    // All domain streams closed (all-streams-close, issue
                    // #134): no domain sender remains anywhere, so the actor
                    // cannot be reached at all — yet the supervisor (holding
                    // only the lifecycle channel) is still alive. This exit is
                    // abnormal *for an operational actor*: report Failed so
                    // the child policy applies (its command send fails Closed
                    // — the task is already gone — and the supervisor records
                    // task-gone). An actor suspended in Init stays silent: it
                    // was properly stopped, and losing its domain senders is
                    // just the app-teardown cascade.
                    if !machine.current_state().is_init() {
                        if let Some(ref notify) = supervisor_notify {
                            report_outcome::<S, R>(&DispatchOutcome::Failed, actor_id, notify);
                        }
                    }
                    Poll::Ready(LoopAction::Stop)
                }
                Poll::Pending => Poll::Pending,
            }
        })
        .await;

        match action {
            LoopAction::Continue => {
                R::yield_now().await;
            }
            LoopAction::Stop => break,
        }
    }
}
