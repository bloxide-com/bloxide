// Copyright 2025 Bloxide, all rights reserved
use alloc::sync::Arc;
use alloc::vec::Vec;
use bloxide_core::{
    capability::{BloxRuntime, KillCap},
    messaging::{ActorId, ActorRef},
};

use bloxide_core::lifecycle::LifecycleCommand;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChildPolicy {
    /// Restart the child up to `max` times. `max` is the number of restart
    /// *attempts* allowed: after the `max`-th restart the next failure triggers
    /// group shutdown. `Restart { max: 0 }` means no restarts — equivalent to `Stop`.
    Restart { max: usize },
    /// Send Stop command for clean shutdown (callbacks run).
    Stop,
    /// Immediately kill the child without callbacks (policy-driven cleanup for dynamic actors).
    /// Only available if KillCap is provided to ChildGroup.
    Kill,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GroupShutdown {
    WhenAnyDone,
    WhenAllDone,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum ChildAction {
    #[default]
    Continue,
    BeginShutdown,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
enum ChildPhase {
    #[default]
    Init,
    Running,
    AwaitingReset,
    PermanentlyDone,
    Stopped,
    Killed,
}

struct ChildEntry<R: BloxRuntime> {
    id: ActorId,
    lifecycle_ref: ActorRef<LifecycleCommand, R>,
    policy: ChildPolicy,
    restarts: usize,
    permanently_done: bool,
    stopped: bool,
    phase: ChildPhase,
    awaiting_alive: bool,
}

pub struct ChildGroup<R: BloxRuntime> {
    children: Vec<ChildEntry<R>>,
    shutdown: GroupShutdown,
    stopped_count: usize,
    /// Optional KillCap for policy-driven cleanup of dynamic actors.
    /// None for Embassy (no task abort), Some for Tokio.
    kill_cap: Option<Arc<dyn KillCap>>,
}

impl<R: BloxRuntime> ChildGroup<R> {
    pub fn new(shutdown: GroupShutdown) -> Self {
        Self {
            children: Vec::new(),
            shutdown,
            stopped_count: 0,
            kill_cap: None,
        }
    }

    /// Create a new ChildGroup with KillCap support for dynamic actor cleanup.
    pub fn with_kill_cap(shutdown: GroupShutdown, kill_cap: Arc<dyn KillCap>) -> Self {
        Self {
            children: Vec::new(),
            shutdown,
            stopped_count: 0,
            kill_cap: Some(kill_cap),
        }
    }

    /// Set the KillCap after construction (for builders that need deferred setup).
    pub fn set_kill_cap(&mut self, kill_cap: Arc<dyn KillCap>) {
        self.kill_cap = Some(kill_cap);
    }

    pub fn add(
        &mut self,
        id: ActorId,
        lifecycle_ref: ActorRef<LifecycleCommand, R>,
        policy: ChildPolicy,
    ) {
        self.children.push(ChildEntry {
            id,
            lifecycle_ref,
            policy,
            restarts: 0,
            permanently_done: false,
            stopped: false,
            phase: ChildPhase::Init,
            awaiting_alive: false,
        });
    }

    pub fn start_child(&self, child_id: ActorId, from: ActorId) {
        if let Some(entry) = self.children.iter().find(|entry| entry.id == child_id) {
            if entry
                .lifecycle_ref
                .try_send(from, LifecycleCommand::Start)
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Start to child {} failed (channel full)",
                    entry.id
                );
            }
        }
    }

    pub fn start_all(&self, from: ActorId) {
        for entry in &self.children {
            if entry
                .lifecycle_ref
                .try_send(from, LifecycleCommand::Start)
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Start to child {} failed (channel full)",
                    entry.id
                );
            }
        }
    }

    pub fn stop_all(&self, from: ActorId) {
        for entry in &self.children {
            if entry
                .lifecycle_ref
                .try_send(from, LifecycleCommand::Stop)
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Stop to child {} failed (channel full)",
                    entry.id
                );
            }
        }
    }

    /// Kill a child immediately using KillCap (no callbacks, no Stop command).
    /// Returns true if the child was killed, false if KillCap unavailable or child not found.
    pub fn kill_child(&mut self, child_id: ActorId) -> bool {
        let Some(kill_cap) = &self.kill_cap else {
            bloxide_log::blox_log_warn!(0, "KillCap not available for child {}", child_id);
            return false;
        };

        let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) else {
            return false;
        };

        if entry.phase == ChildPhase::Killed || entry.stopped {
            return false; // Already killed or stopped
        }

        kill_cap.kill(child_id);
        entry.phase = ChildPhase::Killed;
        entry.stopped = true;
        self.stopped_count += 1;
        true
    }

    pub fn handle_done_or_failed(&mut self, child_id: ActorId, from: ActorId) -> ChildAction {
        let idx = match self.children.iter().position(|e| e.id == child_id) {
            Some(idx) => idx,
            None => return ChildAction::Continue,
        };

        // Extract values needed for decision-making to avoid borrow conflicts
        let (phase, policy, restarts) = {
            let entry = &self.children[idx];
            (entry.phase, entry.policy, entry.restarts)
        };

        if matches!(
            phase,
            ChildPhase::AwaitingReset
                | ChildPhase::PermanentlyDone
                | ChildPhase::Stopped
                | ChildPhase::Killed
        ) {
            return ChildAction::Continue;
        }

        // Handle Kill policy - immediately kill the child
        if policy == ChildPolicy::Kill {
            let killed = self.kill_child(child_id);
            if killed {
                return self.check_shutdown();
            }
            // Fall through if kill failed (no KillCap)
        }

        if let ChildPolicy::Restart { max } = policy {
            if restarts < max {
                if self.children[idx]
                    .lifecycle_ref
                    .try_send(from, LifecycleCommand::Reset)
                    .is_err()
                {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send Reset to child {} failed (channel full)",
                        self.children[idx].id
                    );
                }
                self.children[idx].phase = ChildPhase::AwaitingReset;
                self.children[idx].awaiting_alive = false;
                return ChildAction::Continue;
            }
        }

        self.children[idx].permanently_done = true;
        self.children[idx].phase = ChildPhase::PermanentlyDone;
        self.children[idx].awaiting_alive = false;

        self.check_shutdown()
    }

    fn check_shutdown(&self) -> ChildAction {
        match self.shutdown {
            GroupShutdown::WhenAnyDone => ChildAction::BeginShutdown,
            GroupShutdown::WhenAllDone => {
                if self
                    .children
                    .iter()
                    .all(|e| e.permanently_done || e.stopped)
                {
                    ChildAction::BeginShutdown
                } else {
                    ChildAction::Continue
                }
            }
        }
    }

    pub fn handle_reset(&mut self, child_id: ActorId, from: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            entry.restarts += 1;
            entry.phase = ChildPhase::Running;
            entry.awaiting_alive = false;
            if entry
                .lifecycle_ref
                .try_send(from, LifecycleCommand::Start)
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Start to child {} failed (channel full)",
                    entry.id
                );
            }
        }
    }

    pub fn handle_started(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            if !matches!(
                entry.phase,
                ChildPhase::PermanentlyDone | ChildPhase::Stopped | ChildPhase::Killed
            ) {
                entry.phase = ChildPhase::Running;
                entry.awaiting_alive = false;
            }
        }
    }

    pub fn handle_alive(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            if !matches!(
                entry.phase,
                ChildPhase::PermanentlyDone | ChildPhase::Stopped | ChildPhase::Killed
            ) {
                entry.awaiting_alive = false;
            }
        }
    }

    pub fn health_check_tick(&mut self, from: ActorId) -> ChildAction {
        let stale_ids: Vec<ActorId> = self
            .children
            .iter()
            .filter(|entry| Self::is_health_monitored(entry) && entry.awaiting_alive)
            .map(|entry| entry.id)
            .collect();

        let mut action = ChildAction::Continue;
        for child_id in stale_ids {
            if self.handle_done_or_failed(child_id, from) == ChildAction::BeginShutdown {
                action = ChildAction::BeginShutdown;
            }
        }

        for entry in &mut self.children {
            if Self::is_health_monitored(entry) {
                if entry
                    .lifecycle_ref
                    .try_send(from, LifecycleCommand::Ping)
                    .is_err()
                {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send Ping to child {} failed (channel full)",
                        entry.id
                    );
                }
                entry.awaiting_alive = true;
            } else {
                entry.awaiting_alive = false;
            }
        }

        action
    }

    fn is_health_monitored(entry: &ChildEntry<R>) -> bool {
        !entry.permanently_done
            && !entry.stopped
            && !matches!(
                entry.phase,
                ChildPhase::AwaitingReset | ChildPhase::PermanentlyDone | ChildPhase::Killed
            )
    }

    pub fn record_stopped(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            if !entry.stopped {
                entry.stopped = true;
                entry.phase = ChildPhase::Stopped;
                entry.awaiting_alive = false;
                self.stopped_count += 1;
            }
        }
    }

    pub fn all_stopped(&self) -> bool {
        self.stopped_count == self.children.len()
    }

    /// Reset all restart and stop counters for a new lifecycle epoch.
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
            entry.restarts = 0;
            entry.permanently_done = false;
            entry.stopped = false;
            entry.phase = ChildPhase::Init;
            entry.awaiting_alive = false;
        }
        self.stopped_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bloxide_core::{capability::DynamicChannelCap, test_utils::TestRuntime};

    fn setup_one_child(
        policy: ChildPolicy,
    ) -> (
        ChildGroup<TestRuntime>,
        bloxide_core::test_utils::TestReceiver<LifecycleCommand>,
    ) {
        let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
        let id = 1usize;
        let (lifecycle_ref, rx) = TestRuntime::channel::<LifecycleCommand>(id, 16);
        group.add(id, lifecycle_ref, policy);
        (group, rx)
    }

    #[test]
    fn duplicate_done_while_awaiting_reset_is_coalesced() {
        let (mut group, mut rx) = setup_one_child(ChildPolicy::Restart { max: 2 });
        let from = 100usize;
        assert_eq!(group.handle_done_or_failed(1, from), ChildAction::Continue);
        let cmds = rx.drain_payloads();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], LifecycleCommand::Reset));

        assert_eq!(group.handle_done_or_failed(1, from), ChildAction::Continue);
        assert!(
            rx.drain_payloads().is_empty(),
            "duplicate Done while awaiting reset must not enqueue another Reset"
        );
    }

    #[test]
    fn health_tick_pings_child_and_marks_missed_alive_as_failed() {
        let (mut group, mut rx) = setup_one_child(ChildPolicy::Restart { max: 1 });
        let from = 100usize;

        assert_eq!(group.health_check_tick(from), ChildAction::Continue);
        let first = rx.drain_payloads();
        assert_eq!(first.len(), 1);
        assert!(matches!(first[0], LifecycleCommand::Ping));

        assert_eq!(group.health_check_tick(from), ChildAction::Continue);
        let second = rx.drain_payloads();
        assert_eq!(
            second.len(),
            1,
            "second tick should emit Reset for a stale child"
        );
        assert!(matches!(second[0], LifecycleCommand::Reset));
    }
}
