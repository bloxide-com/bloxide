// Copyright 2025 Bloxide, all rights reserved
//! Child group tracking, reset policies, and health checking.
//!
//! This is the reusable platform primitive for managing supervised children.
//! It is not supervisor-specific — any blox that tracks child actors can use
//! `ChildGroup` directly. The supervisor is one such consumer; a custom
//! managing blox could use it without depending on `bloxide-supervisor`.
//!
//! Following the Platform Feature Pattern (spec 20), this crate also owns the
//! child-management control plane (`control`: `ChildCtrl`, `RegisterChild`,
//! `RegisterDynamicChild`) and the consumer-side action functions (`actions`)
//! that a managing blox wires into its topology.
//!
//! # Reliability contract: confirm-before-record
//!
//! Every control-plane send is `try_send`, and the error kind is
//! load-bearing:
//!
//! - **Closed** is definitive: the child's task is gone. The entry moves to
//!   the terminal `Gone` phase immediately — dead-channel evidence needs no
//!   acknowledgment.
//! - **Full** is transient: the entry keeps its current phase and the command
//!   is stashed in `pending_cmd`. `flush_pending` retries it on every event
//!   pass through the group, so a lost command is recovered as long as the
//!   managing blox keeps receiving events.
//!
//! Phase transitions happen only on confirmed delivery or on observed
//! lifecycle reports — never on attempted sends. Bookkeeping therefore never
//! diverges from reality by more than one in-flight command.
//!
//! Note on runtimes: Embassy's static channels never close, so the Closed
//! branch is exercised on Tokio/TestRuntime; on Embassy a dead child is
//! detected via health-check misses instead.

#![no_std]
extern crate alloc;

pub mod actions;
pub mod builder;
pub mod control;

use alloc::vec::Vec;
use bloxide_core::{
    capability::{BloxRuntime, KillCapability},
    lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand},
    messaging::{ActorId, ActorRef},
};

/// Consecutive unanswered Pings before a child is declared rogue.
const MAX_MISSES: u8 = 2;

/// Supervision policy for a child actor.
///
/// Determines what the managing blox does when the child fails (reports
/// `Stopped` or `Failed`, or misses `MAX_MISSES` consecutive health checks).
///
/// The four-level lifecycle model (`reset → stop → abort → kill`):
///
/// | Policy | Mechanism | Cooperative? | Callbacks? | Revivable? |
/// |--------|-----------|-------------|------------|------------|
/// | `Reset { .. }` | Send `Reset` | Yes | Exit + entry chain | Yes (immediately) |
/// | `Stop` | No command — child marked done for this epoch | — | — | No |
/// | `Abort` | Send `AbortCommand` on abort mailbox | Yes (cooperative) | None | Yes (respawn task) |
/// | `Kill` | `KillCapability::kill(handle)` | No (forced) | None | No (permanently dead) |
///
/// `Abort` and `Kill` require a dynamically spawned child (registered via
/// `ChildGroup::try_add_dynamic` with abort/kill handles); `try_add` rejects
/// them with `RegistrationError::PolicyRequiresHandles`. `Kill` additionally
/// requires a runtime with `KillCapability::CAN_KILL` (`NoKill` runtimes such
/// as Embassy reject it with `RegistrationError::KillUnavailable`) — killing
/// is a no-op there, and marking a live child `Killed` would corrupt the
/// group's bookkeeping.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChildPolicy {
    /// Send `Reset` to the child — goes directly to `initial_state()`.
    /// The actor is immediately operational. No need to send `Start` separately.
    /// The child revives and continues running.
    ///
    /// `max` caps **consecutive** restarts: the counter increments on each
    /// delivered `Reset` and resets to zero when the child reports `Started`
    /// and then answers a health `Ping` with `Alive` (a no-clock proxy for
    /// sustained uptime). Once the child has been reset `max` times in a row,
    /// the next failure gives up: the child is marked `Stopped` (terminal,
    /// task alive) and group shutdown is evaluated. Without watchdog ticks,
    /// `Alive` never arrives, so `max` degrades to a lifetime restart cap —
    /// the fail-safe direction.
    Reset { max: u32 },
    /// Leave the child as-is and mark it done for this epoch (counts toward
    /// group shutdown). No command is sent: the child already self-stopped
    /// (suspended in Init) or failed (parked in its error state).
    Stop,
    /// Send `AbortCommand` on the abort mailbox for cooperative self-termination.
    /// No callbacks fire. The child's task ends. Requires the child to have
    /// an abort capability mailbox.
    Abort,
    /// Immediately kill the child via `KillCapability::kill(handle)`.
    /// The task is destroyed externally — no callbacks, no cooperation.
    /// Permanently dead. Requires the child to have a kill capability
    /// (kill handle from `SpawnCap`) and a runtime with `CAN_KILL`.
    Kill,
}

/// When to trigger group-level shutdown.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GroupShutdown {
    WhenAnyDone,
    WhenAllDone,
}

// Re-export the generic builder
pub use builder::ChildGroupBuilder;

// Re-export the control-plane message types
pub use control::{ChildCtrl, RegisterChild, RegisterDynamicChild};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum ChildAction {
    #[default]
    Continue,
    BeginShutdown,
}

/// Why a registration was rejected. Registration is fallible everywhere —
/// the managing blox's action functions warn-and-drop on `Err` instead of
/// panicking (a bad `ChildCtrl` message must not reset an MCU).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RegistrationError {
    /// `ChildPolicy::Abort`/`Kill` requested for a static child (no abort
    /// mailbox, no kill handle). Register via `try_add_dynamic` or use
    /// `Reset`/`Stop`.
    PolicyRequiresHandles,
    /// `ChildPolicy::Kill` on a runtime whose `KillCapability` is a no-op
    /// (`!CAN_KILL`, e.g. Embassy). Killing would not work and the group
    /// would wrongly mark a live child dead.
    KillUnavailable,
    /// A child with this `ActorId` is already registered.
    Duplicate,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
enum ChildPhase {
    #[default]
    Init,
    Running,
    /// Reset was confirmed sent; waiting for the child to report Started.
    ResetPending,
    /// `AbortCommand` was confirmed sent; waiting for the child's run loop to
    /// report `Aborted`. The task is expected to end on its own.
    Aborting,
    /// Child is done for this epoch but its task is alive: self-stopped
    /// (suspended in Init), failed (parked in its error state), restart cap
    /// exhausted, or marked done by the `Stop` policy. Terminal — it will
    /// not revive on its own.
    Stopped,
    /// Child was aborted (cooperative self-termination via `AbortCommand`).
    /// The task is gone; restarting requires respawning it.
    Aborted,
    /// Child was killed (forced destruction via `KillCapability::kill`).
    /// The task is gone; permanently dead.
    Killed,
    /// The child's channel was observed Closed — definitive evidence the
    /// task is gone. Terminal. (Unreachable on Embassy, whose channels never
    /// close; health-check misses cover it there.)
    Gone,
}

impl ChildPhase {
    /// Terminal for this epoch: the child will not revive on its own.
    /// `Stopped` children still have a live (suspended/parked) task;
    /// `Aborted`/`Killed`/`Gone` children do not.
    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Stopped | Self::Aborted | Self::Killed | Self::Gone
        )
    }

    /// The child's task is gone — its mailboxes are dead, so lifecycle
    /// commands (e.g. `stop_all`) must not be sent to it.
    fn is_task_gone(self) -> bool {
        matches!(self, Self::Aborted | Self::Killed | Self::Gone)
    }
}

/// A lifecycle command that could not be delivered yet (channel Full) and is
/// retried by `flush_pending` on every event pass. `Kill` never appears here
/// — it is synchronous.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PendingCmd {
    Start,
    Stop,
    Reset,
    Abort,
}

struct ChildEntry<R: BloxRuntime> {
    id: ActorId,
    lifecycle_ref: ActorRef<LifecycleCommand, R>,
    policy: ChildPolicy,
    phase: ChildPhase,
    /// A Ping was delivered and has not been answered by `Alive` yet.
    ping_outstanding: bool,
    /// Consecutive unanswered Pings. At `MAX_MISSES` the child is rogue.
    misses: u8,
    /// Consecutive delivered Resets since the child last proved sustained
    /// uptime (`Alive`). Compared against `ChildPolicy::Reset { max }`.
    restarts: u32,
    /// Undelivered lifecycle command awaiting retry (see `flush_pending`).
    pending_cmd: Option<PendingCmd>,
    /// Abort capability mailbox (send side). `None` for static children
    /// registered via `RegisterChild` (no abort capability).
    abort_ref: Option<ActorRef<AbortCommand, R>>,
    /// Cloneable kill handle for external task kill (the ripcord). `None` for
    /// static children. Consumed by `R::Kill::kill(handle)` when
    /// `ChildPolicy::Kill` fires.
    /// This is `R::KillHandle` (Clone), not `R::TaskHandle` (not Clone).
    kill_handle: Option<<R::Kill as KillCapability<R>>::Handle>,
}

pub struct ChildGroup<R: BloxRuntime> {
    children: Vec<ChildEntry<R>>,
    shutdown: GroupShutdown,
}

impl<R: BloxRuntime> ChildGroup<R> {
    pub fn new(shutdown: GroupShutdown) -> Self {
        Self {
            children: Vec::new(),
            shutdown,
        }
    }

    /// Register a static child (no abort/kill capability).
    ///
    /// Fallible: `Abort`/`Kill` policies need handles (`try_add_dynamic`),
    /// and duplicate ids are rejected.
    pub fn try_add(
        &mut self,
        id: ActorId,
        lifecycle_ref: ActorRef<LifecycleCommand, R>,
        policy: ChildPolicy,
    ) -> Result<(), RegistrationError> {
        if matches!(policy, ChildPolicy::Kill | ChildPolicy::Abort) {
            return Err(RegistrationError::PolicyRequiresHandles);
        }
        if self.children.iter().any(|e| e.id == id) {
            return Err(RegistrationError::Duplicate);
        }
        self.children.push(ChildEntry {
            id,
            lifecycle_ref,
            policy,
            phase: ChildPhase::Init,
            ping_outstanding: false,
            misses: 0,
            restarts: 0,
            pending_cmd: None,
            abort_ref: None,
            kill_handle: None,
        });
        Ok(())
    }

    /// Register a dynamically spawned child that has an abort capability.
    ///
    /// Stores the `abort_ref` (for cooperative self-termination via the abort
    /// mailbox) and the `kill_handle` (for the external kill ripcord) so the
    /// supervisor can abort or kill the child when policy dictates.
    ///
    /// Fallible: `ChildPolicy::Kill` requires `KillCapability::CAN_KILL`
    /// (refused on `NoKill` runtimes — the kill would be a no-op and the
    /// group would mark a live child dead), and duplicate ids are rejected.
    pub fn try_add_dynamic(
        &mut self,
        id: ActorId,
        lifecycle_ref: ActorRef<LifecycleCommand, R>,
        abort_ref: ActorRef<AbortCommand, R>,
        kill_handle: <R::Kill as KillCapability<R>>::Handle,
        policy: ChildPolicy,
    ) -> Result<(), RegistrationError> {
        if policy == ChildPolicy::Kill && !<R::Kill as KillCapability<R>>::CAN_KILL {
            return Err(RegistrationError::KillUnavailable);
        }
        if self.children.iter().any(|e| e.id == id) {
            return Err(RegistrationError::Duplicate);
        }
        self.children.push(ChildEntry {
            id,
            lifecycle_ref,
            policy,
            phase: ChildPhase::Init,
            ping_outstanding: false,
            misses: 0,
            restarts: 0,
            pending_cmd: None,
            abort_ref: Some(abort_ref),
            kill_handle: Some(kill_handle),
        });
        Ok(())
    }

    /// Send `Start` to one child, confirm-before-record: `Full` queues the
    /// command in `pending_cmd` for later flush; `Closed` marks the child
    /// `Gone` (its task is provably dead).
    pub fn start_child(&mut self, child_id: ActorId, from: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|entry| entry.id == child_id) {
            match entry.lifecycle_ref.try_send(from, LifecycleCommand::Start) {
                Ok(()) => {}
                Err(e) if R::try_send_error_is_closed(&e) => {
                    Self::mark_gone(entry, from, "Start");
                }
                Err(_) => {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send Start to child {} failed (channel full) — queued for retry",
                        entry.id
                    );
                    entry.pending_cmd = Some(PendingCmd::Start);
                }
            }
        }
    }

    /// Start all non-terminal children. Terminal children are skipped:
    /// their epoch is accounted as over, and reviving a `Stopped` child's
    /// task while the group still records it terminal would desynchronize
    /// the bookkeeping (the `Started` ack is guarded out for terminal
    /// phases). Task-gone phases have dead mailboxes — doubly skipped.
    pub fn start_all(&mut self, from: ActorId) {
        for entry in &mut self.children {
            if entry.phase.is_terminal() {
                continue;
            }
            match entry.lifecycle_ref.try_send(from, LifecycleCommand::Start) {
                Ok(()) => {}
                Err(e) if R::try_send_error_is_closed(&e) => {
                    Self::mark_gone(entry, from, "Start");
                }
                Err(_) => {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send Start to child {} failed (channel full) — queued for retry",
                        entry.id
                    );
                    entry.pending_cmd = Some(PendingCmd::Start);
                }
            }
        }
    }

    pub fn stop_all(&mut self, from: ActorId) {
        for entry in &mut self.children {
            // Skip children whose task is gone (Aborted/Killed/Gone) — their
            // mailboxes are dead; sending Stop would only log warnings.
            // Stopped children still get Stop: their task is alive
            // (suspended in Init or failed-parked) and acknowledges with
            // Stopped (the engine acks Stop-in-Init).
            if entry.phase.is_task_gone() {
                continue;
            }
            match entry.lifecycle_ref.try_send(from, LifecycleCommand::Stop) {
                Ok(()) => {}
                Err(e) if R::try_send_error_is_closed(&e) => {
                    Self::mark_gone(entry, from, "Stop");
                }
                Err(_) => {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send Stop to child {} failed (channel full) — queued for retry",
                        entry.id
                    );
                    entry.pending_cmd = Some(PendingCmd::Stop);
                }
            }
        }
    }

    /// Record task-gone from a Closed send observation. Silent when the entry
    /// is already terminal (expected shutdown race — e.g. a `Done` child's
    /// deregistration dropped its channels); warns otherwise.
    fn mark_gone(entry: &mut ChildEntry<R>, from: ActorId, cmd: &'static str) {
        if entry.phase.is_terminal() {
            return;
        }
        bloxide_log::blox_log_warn!(
            from,
            "try_send {} to child {} failed (channel closed) — child task is gone",
            cmd,
            entry.id
        );
        entry.phase = ChildPhase::Gone;
        entry.ping_outstanding = false;
        entry.pending_cmd = None;
    }

    /// Retry every queued `pending_cmd`. Called on every event pass through
    /// the group (wired as the first action on the managing blox's
    /// transitions), so a command lost to a full channel is recovered as long
    /// as the managing blox keeps receiving events.
    ///
    /// Delivery is confirmed the same way as the original send:
    /// - Ok — the command's own bookkeeping now applies (Reset →
    ///   `ResetPending` + restart counted; Abort → `Aborting`; Start/Stop →
    ///   nothing — the child's report drives the phase).
    /// - Full — stays queued.
    /// - Closed — the child is `Gone`.
    ///
    /// Terminal entries drop their queued command (a pending remedy is moot
    /// once the child has stopped/ended).
    pub fn flush_pending(&mut self, from: ActorId) -> ChildAction {
        let mut any_gone = false;
        for entry in &mut self.children {
            let Some(cmd) = entry.pending_cmd else {
                continue;
            };
            if entry.phase.is_terminal() {
                entry.pending_cmd = None;
                continue;
            }
            let send_result = match cmd {
                PendingCmd::Start | PendingCmd::Stop | PendingCmd::Reset => {
                    let lc = match cmd {
                        PendingCmd::Start => LifecycleCommand::Start,
                        PendingCmd::Stop => LifecycleCommand::Stop,
                        _ => LifecycleCommand::Reset,
                    };
                    entry.lifecycle_ref.try_send(from, lc)
                }
                PendingCmd::Abort => match &entry.abort_ref {
                    Some(abort_ref) => {
                        abort_ref.try_send(from, AbortCommand::Abort { child_id: entry.id })
                    }
                    None => {
                        // No abort mailbox (should not happen — Abort is only
                        // queued for dynamic children). Drop the command.
                        entry.pending_cmd = None;
                        continue;
                    }
                },
            };
            match send_result {
                Ok(()) => {
                    entry.pending_cmd = None;
                    match cmd {
                        PendingCmd::Reset => {
                            entry.phase = ChildPhase::ResetPending;
                            entry.restarts += 1;
                        }
                        PendingCmd::Abort => {
                            entry.phase = ChildPhase::Aborting;
                        }
                        // Start/Stop: the child's report (Started/Stopped)
                        // drives the phase — nothing to record on send.
                        PendingCmd::Start | PendingCmd::Stop => {}
                    }
                }
                Err(e) if R::try_send_error_is_closed(&e) => {
                    entry.pending_cmd = None;
                    if !entry.phase.is_terminal() {
                        entry.phase = ChildPhase::Gone;
                        entry.ping_outstanding = false;
                        any_gone = true;
                        bloxide_log::blox_log_warn!(
                            from,
                            "flush of pending command to child {} failed (channel closed) — child task is gone",
                            entry.id
                        );
                    }
                }
                Err(_) => {
                    // Still full — stays queued for the next event pass.
                }
            }
        }
        if any_gone {
            self.check_shutdown()
        } else {
            ChildAction::Continue
        }
    }

    /// Handle a `Stopped` or `Failed` lifecycle event for a child.
    ///
    /// Applies the child's `ChildPolicy`, confirm-before-record:
    /// - `Reset { max }` → if the consecutive-restart cap is exhausted, give
    ///   up (`Stopped`, terminal) and evaluate shutdown. Otherwise send
    ///   `Reset`: Ok → `ResetPending`; Full → queued in `pending_cmd`;
    ///   Closed → `Gone`.
    /// - `Stop` → set `Stopped` (task alive, terminal for epoch) → `check_shutdown()`
    /// - `Abort` → send `AbortCommand`: Ok → `Aborting` (the `Aborted` report
    ///   finalizes); Full → queued; Closed → `Gone`.
    /// - `Kill` → `KillCapability::kill(handle)` (synchronous), set `Killed`
    ///   → `check_shutdown()`. Only reachable on `CAN_KILL` runtimes —
    ///   enforced at registration.
    ///
    /// Children with a queued policy remedy (`pending_cmd` Reset/Abort), an
    /// in-flight transition (`ResetPending`/`Aborting`), or a terminal phase
    /// are coalesced: the policy is already in motion.
    pub fn handle_done_or_failed(
        &mut self,
        child_id: ActorId,
        from: ActorId,
        notify: &ActorRef<ChildLifecycleEvent, R>,
    ) -> ChildAction {
        let idx = match self.children.iter().position(|e| e.id == child_id) {
            Some(idx) => idx,
            None => {
                bloxide_log::blox_log_warn!(
                    from,
                    "lifecycle event for unknown child {} — ignored",
                    child_id
                );
                return ChildAction::Continue;
            }
        };

        // Extract values needed for decision-making to avoid borrow conflicts
        let (phase, policy, has_pending_remedy) = {
            let entry = &self.children[idx];
            (
                entry.phase,
                entry.policy,
                matches!(
                    entry.pending_cmd,
                    Some(PendingCmd::Reset | PendingCmd::Abort)
                ),
            )
        };

        if phase.is_terminal()
            || phase == ChildPhase::ResetPending
            || phase == ChildPhase::Aborting
            || has_pending_remedy
        {
            return ChildAction::Continue;
        }

        // Handle Kill policy: call R::Kill::kill(kill_handle) — the ripcord.
        // This immediately terminates the child — no callbacks fire, no
        // cooperative shutdown. Permanently dead. Registration guarantees
        // `CAN_KILL` and a stored handle, so the kill is real here.
        if policy == ChildPolicy::Kill {
            // Take the kill_handle out — kill() consumes it by value.
            let kill_handle = self.children[idx].kill_handle.take();
            if let Some(handle) = kill_handle {
                R::Kill::kill(handle);
            }

            // Emit the Killed lifecycle event so the supervisor (and any
            // observers on the notify channel) learn the child was killed.
            // The kill is synchronous, so we emit the event directly (the
            // killed task never runs again to report anything).
            if notify
                .try_send(from, ChildLifecycleEvent::Killed { child_id })
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Killed to supervisor for child {} failed (channel full or closed)",
                    child_id
                );
            }

            self.children[idx].phase = ChildPhase::Killed;
            self.children[idx].ping_outstanding = false;
            return self.check_shutdown();
        }

        // Handle Abort policy: send AbortCommand on the abort mailbox.
        // The child's task self-terminates cooperatively (no callbacks) and
        // its run loop reports `Aborted` — that report finalizes the phase.
        if policy == ChildPolicy::Abort {
            let send_result = match &self.children[idx].abort_ref {
                Some(abort_ref) => abort_ref.try_send(from, AbortCommand::Abort { child_id }),
                None => unreachable!("Abort policy requires abort_ref — checked at registration"),
            };
            match send_result {
                Ok(()) => {
                    self.children[idx].phase = ChildPhase::Aborting;
                    self.children[idx].ping_outstanding = false;
                    return ChildAction::Continue;
                }
                Err(e) if R::try_send_error_is_closed(&e) => {
                    Self::mark_gone(&mut self.children[idx], from, "Abort");
                    return self.check_shutdown();
                }
                Err(_) => {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send AbortCommand::Abort to child {} failed (channel full) — queued for retry",
                        child_id
                    );
                    self.children[idx].pending_cmd = Some(PendingCmd::Abort);
                    return ChildAction::Continue;
                }
            }
        }

        // Handle Reset policy, capped at `max` consecutive restarts.
        if let ChildPolicy::Reset { max } = policy {
            if self.children[idx].restarts >= max {
                bloxide_log::blox_log_warn!(
                    from,
                    "child {} exhausted {} consecutive restarts — giving up",
                    child_id,
                    max
                );
                self.children[idx].phase = ChildPhase::Stopped;
                self.children[idx].ping_outstanding = false;
                return self.check_shutdown();
            }
            match self.children[idx]
                .lifecycle_ref
                .try_send(from, LifecycleCommand::Reset)
            {
                Ok(()) => {
                    self.children[idx].phase = ChildPhase::ResetPending;
                    self.children[idx].restarts += 1;
                    self.children[idx].ping_outstanding = false;
                }
                Err(e) if R::try_send_error_is_closed(&e) => {
                    Self::mark_gone(&mut self.children[idx], from, "Reset");
                    return self.check_shutdown();
                }
                Err(_) => {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send Reset to child {} failed (channel full) — queued for retry",
                        child_id
                    );
                    self.children[idx].pending_cmd = Some(PendingCmd::Reset);
                }
            }
            return ChildAction::Continue;
        }

        // Handle Stop policy: the child is already stopping (Stopped event)
        // or has failed. Mark Stopped and check group shutdown.
        // Stop means the child goes to Init, suspended (task alive) — it can
        // be restarted with `Start` later, but from the ChildGroup's
        // perspective it is done for this epoch (terminal).
        self.children[idx].phase = ChildPhase::Stopped;
        self.children[idx].ping_outstanding = false;

        self.check_shutdown()
    }

    fn check_shutdown(&self) -> ChildAction {
        match self.shutdown {
            GroupShutdown::WhenAnyDone => ChildAction::BeginShutdown,
            GroupShutdown::WhenAllDone => {
                // Empty group: vacuously true (Done-deregistration shutdown).
                if self.children.iter().all(|e| e.phase.is_terminal()) {
                    ChildAction::BeginShutdown
                } else {
                    ChildAction::Continue
                }
            }
        }
    }

    /// Handle a `ChildLifecycleEvent::Started` for a child.
    ///
    /// In the four-level lifecycle model, `Started` is sent for both initial
    /// `Start` and `Reset` (both go directly to `initial_state()`). The
    /// supervisor does not need to send `Start` after `Reset` — `Reset` is
    /// self-contained.
    ///
    /// A `Started` report also clears health-miss state (the child is
    /// demonstrably alive) but deliberately does NOT reset the consecutive-
    /// restart counter — that requires surviving a full watchdog tick (`Alive`),
    /// otherwise a crash loop would keep resetting its own cap.
    pub fn handle_started(&mut self, child_id: ActorId, from: ActorId) {
        match self.children.iter_mut().find(|e| e.id == child_id) {
            Some(entry) if !entry.phase.is_terminal() => {
                entry.phase = ChildPhase::Running;
                entry.ping_outstanding = false;
                entry.misses = 0;
            }
            Some(_) => {}
            None => {
                bloxide_log::blox_log_warn!(
                    from,
                    "Started for unknown child {} — ignored",
                    child_id
                );
            }
        }
    }

    /// Handle a `ChildLifecycleEvent::Alive` for a child.
    ///
    /// `Alive` is operational evidence (the engine only answers `Ping` from a
    /// non-error operational state), so it heals lost `Started` reports: an
    /// entry still in `Init` or `ResetPending` moves to `Running`. It also
    /// proves sustained uptime, so the consecutive-restart counter resets.
    ///
    /// A late `Alive` from an unknown or terminal child is a normal race
    /// (deregistered or stopped between Ping and reply) — absorbed silently.
    pub fn handle_alive(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            if entry.phase.is_terminal() {
                return;
            }
            entry.ping_outstanding = false;
            entry.misses = 0;
            entry.restarts = 0;
            if matches!(entry.phase, ChildPhase::Init | ChildPhase::ResetPending) {
                entry.phase = ChildPhase::Running;
            }
        }
    }

    pub fn watchdog_tick(
        &mut self,
        from: ActorId,
        notify: &ActorRef<ChildLifecycleEvent, R>,
    ) -> ChildAction {
        // 1. Verdict pass: children with a Ping still outstanding from a
        // previous tick have missed it. `MAX_MISSES` consecutive misses →
        // rogue → normal child policy applies.
        let mut rogue_ids: Vec<ActorId> = Vec::new();
        for entry in &mut self.children {
            if !Self::is_health_monitored(entry) {
                continue;
            }
            if entry.ping_outstanding {
                entry.misses += 1;
                if entry.misses >= MAX_MISSES {
                    entry.ping_outstanding = false;
                    entry.misses = 0;
                    rogue_ids.push(entry.id);
                }
            } else {
                entry.misses = 0;
            }
        }

        let mut action = ChildAction::Continue;
        for child_id in rogue_ids {
            if self.handle_done_or_failed(child_id, from, notify) == ChildAction::BeginShutdown {
                action = ChildAction::BeginShutdown;
            }
        }

        // 2. Ping pass: monitored children get one Ping. Confirm-before-
        // record: only a delivered Ping is outstanding (a Full channel is not
        // a miss — the child was never asked); Closed marks the child Gone.
        let mut any_gone = false;
        for entry in &mut self.children {
            if !Self::is_health_monitored(entry) {
                continue;
            }
            match entry.lifecycle_ref.try_send(from, LifecycleCommand::Ping) {
                Ok(()) => {
                    entry.ping_outstanding = true;
                }
                Err(e) if R::try_send_error_is_closed(&e) => {
                    if !entry.phase.is_terminal() {
                        entry.phase = ChildPhase::Gone;
                        entry.ping_outstanding = false;
                        any_gone = true;
                        bloxide_log::blox_log_warn!(
                            from,
                            "try_send Ping to child {} failed (channel closed) — child task is gone",
                            entry.id
                        );
                    }
                }
                Err(_) => {
                    // Channel full — the Ping was not delivered, so this is
                    // not a miss. Any previously outstanding Ping stays
                    // outstanding and is judged on the next tick.
                }
            }
        }

        if any_gone && self.check_shutdown() == ChildAction::BeginShutdown {
            action = ChildAction::BeginShutdown;
        }
        action
    }

    fn is_health_monitored(entry: &ChildEntry<R>) -> bool {
        // Init/Running/ResetPending children are monitored. Terminal phases
        // are done for the epoch; `Aborting` children are expected to end and
        // are awaited via the `Aborted` report (wait-forever by design).
        // ResetPending children are pinged: an `Alive` heals a lost `Started`
        // report, and silence means the Reset never revived them.
        matches!(
            entry.phase,
            ChildPhase::Init | ChildPhase::Running | ChildPhase::ResetPending
        )
    }

    /// Record a `Stopped` report. Idempotent via phase, and never overwrites
    /// a terminal phase: a late `Stopped` must not resurrect mailbox sends to
    /// a dead task.
    pub fn record_stopped(&mut self, child_id: ActorId, from: ActorId) {
        match self.children.iter_mut().find(|e| e.id == child_id) {
            Some(entry) if !entry.phase.is_terminal() => {
                entry.phase = ChildPhase::Stopped;
                entry.ping_outstanding = false;
                entry.pending_cmd = None;
            }
            Some(_) => {}
            None => {
                bloxide_log::blox_log_warn!(
                    from,
                    "Stopped for unknown child {} — ignored",
                    child_id
                );
            }
        }
    }

    /// Record a `Failed` report received while the managing blox is shutting
    /// down. The child is parked in its absorbing error state (task alive),
    /// so it counts as `Stopped` — terminal, task alive. (In `Running`,
    /// `Failed` goes through `handle_done_or_failed` instead, applying the
    /// child policy.)
    pub fn record_failed(&mut self, child_id: ActorId, from: ActorId) {
        match self.children.iter_mut().find(|e| e.id == child_id) {
            Some(entry) if !entry.phase.is_terminal() => {
                entry.phase = ChildPhase::Stopped;
                entry.ping_outstanding = false;
                entry.pending_cmd = None;
            }
            Some(_) => {}
            None => {
                bloxide_log::blox_log_warn!(
                    from,
                    "Failed for unknown child {} — ignored",
                    child_id
                );
            }
        }
    }

    /// Record that a child was aborted (cooperative self-termination via
    /// `AbortCommand`). The child's task has ended. Returns the group
    /// shutdown decision: externally-originated aborts (not just policy-
    /// driven ones) count toward group shutdown.
    pub fn record_aborted(&mut self, child_id: ActorId, from: ActorId) -> ChildAction {
        match self.children.iter_mut().find(|e| e.id == child_id) {
            Some(entry) if !entry.phase.is_terminal() => {
                entry.phase = ChildPhase::Aborted;
                entry.ping_outstanding = false;
                entry.pending_cmd = None;
                self.check_shutdown()
            }
            Some(_) => ChildAction::Continue,
            None => {
                bloxide_log::blox_log_warn!(
                    from,
                    "Aborted for unknown child {} — ignored",
                    child_id
                );
                ChildAction::Continue
            }
        }
    }

    /// Record that a child was killed (external task destruction via
    /// `KillCapability::kill`). The child's task is gone. Permanently dead.
    /// Returns the group shutdown decision, like `record_aborted`.
    pub fn record_killed(&mut self, child_id: ActorId, from: ActorId) -> ChildAction {
        match self.children.iter_mut().find(|e| e.id == child_id) {
            Some(entry) if !entry.phase.is_terminal() => {
                entry.phase = ChildPhase::Killed;
                entry.ping_outstanding = false;
                entry.pending_cmd = None;
                self.check_shutdown()
            }
            Some(_) => ChildAction::Continue,
            None => {
                bloxide_log::blox_log_warn!(
                    from,
                    "Killed for unknown child {} — ignored",
                    child_id
                );
                ChildAction::Continue
            }
        }
    }

    pub fn all_stopped(&self) -> bool {
        // Empty group: vacuously true (Done-deregistration shutdown).
        self.children.iter().all(|e| e.phase.is_terminal())
    }

    /// Deregister a child that self-terminated cleanly
    /// (`ChildLifecycleEvent::Done` — normal completion).
    ///
    /// The entry is removed from the group, dropping its refs so the child's
    /// channels can close. No restart policy is applied — Done is success,
    /// not a fault. Returns the group shutdown decision **only when an entry
    /// was actually removed**: a `Done` from an unknown child is a wiring or
    /// messaging bug and must not trigger shutdown (it warns instead).
    pub fn deregister(&mut self, child_id: ActorId, from: ActorId) -> ChildAction {
        match self.children.iter().position(|e| e.id == child_id) {
            Some(idx) => {
                self.children.remove(idx);
                self.check_shutdown()
            }
            None => {
                bloxide_log::blox_log_warn!(from, "Done for unknown child {} — ignored", child_id);
                ChildAction::Continue
            }
        }
    }

    /// Reset all non-terminal phases for a new lifecycle epoch.
    ///
    /// Terminal entries (`Stopped`/`Aborted`/`Killed`/`Gone`) keep their
    /// phase. The consecutive-restart counter is kept too: it guards against
    /// crash loops, and a managing-blox reset must not bypass the cap.
    ///
    /// # Warning
    ///
    /// On runtimes whose per-child lifecycle channels persist across epochs
    /// (including Embassy's static-channel setup), stale commands queued before
    /// this reset may be delivered to children after the next `start_all`.
    /// Callers must ensure child tasks have consumed all previously queued
    /// commands before calling `clear_counters`.
    pub fn clear_counters(&mut self) {
        for entry in &mut self.children {
            // Terminal entries keep their phase: resetting them to Init would
            // make them health-monitored again — a Ping to a dead mailbox
            // (Aborted/Killed/Gone) or a suspended child (Stopped), and a
            // spurious second Kill when the Ping goes unanswered.
            if entry.phase.is_terminal() {
                continue;
            }
            entry.phase = ChildPhase::Init;
            entry.ping_outstanding = false;
            entry.misses = 0;
            entry.pending_cmd = None;
        }
    }
}

#[cfg(test)]
mod tests;
