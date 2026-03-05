use alloc::vec::Vec;
use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
};

use crate::lifecycle::LifecycleCommand;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChildPolicy {
    /// Restart the child up to `max` times. `max` is the number of restart
    /// *attempts* allowed: after the `max`-th restart the next failure triggers
    /// group shutdown. `Restart { max: 0 }` means no restarts — equivalent to `Stop`.
    Restart {
        max: usize,
    },
    Stop,
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

struct ChildEntry<R: BloxRuntime> {
    id: ActorId,
    lifecycle_ref: ActorRef<LifecycleCommand, R>,
    policy: ChildPolicy,
    restarts: usize,
    permanently_done: bool,
    stopped: bool,
}

pub struct ChildGroup<R: BloxRuntime> {
    children: Vec<ChildEntry<R>>,
    shutdown: GroupShutdown,
    stopped_count: usize,
}

impl<R: BloxRuntime> ChildGroup<R> {
    pub fn new(shutdown: GroupShutdown) -> Self {
        Self {
            children: Vec::new(),
            shutdown,
            stopped_count: 0,
        }
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
        });
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

    pub fn handle_done_or_failed(&mut self, child_id: ActorId, from: ActorId) -> ChildAction {
        let idx = match self.children.iter().position(|e| e.id == child_id) {
            Some(idx) => idx,
            None => return ChildAction::Continue,
        };

        let entry = &self.children[idx];
        if let ChildPolicy::Restart { max } = entry.policy {
            if entry.restarts < max {
                if entry
                    .lifecycle_ref
                    .try_send(from, LifecycleCommand::Terminate)
                    .is_err()
                {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send Terminate to child {} failed (channel full)",
                        entry.id
                    );
                }
                return ChildAction::Continue;
            }
        }

        self.children[idx].permanently_done = true;

        match self.shutdown {
            GroupShutdown::WhenAnyDone => ChildAction::BeginShutdown,
            GroupShutdown::WhenAllDone => {
                if self.children.iter().all(|e| e.permanently_done) {
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

    pub fn record_stopped(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            if !entry.stopped {
                entry.stopped = true;
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
    /// On Embassy (and runtimes using static channels), the per-child
    /// `lifecycle_ref` channels are never drained between lifecycles. Stale
    /// commands queued before this reset may be delivered to children after the
    /// next `start_all`. Callers must ensure child tasks have consumed all
    /// previously queued commands before calling `clear_counters`.
    pub fn clear_counters(&mut self) {
        for entry in &mut self.children {
            entry.restarts = 0;
            entry.permanently_done = false;
            entry.stopped = false;
        }
        self.stopped_count = 0;
    }
}
