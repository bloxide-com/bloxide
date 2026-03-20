// Copyright 2025 Bloxide, all rights reserved
//! Concrete runtime-specific worker factory for the Tokio pool demo.
//!
//! This crate is the impl layer for `tokio-pool-demo`. It is the only place
//! that knows about concrete worker context/spec types and task spawning.

extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
    run_actor_auto_start, StateMachine,
};
use bloxide_spawn::{
    SpawnCap,
};
use bloxide_tokio::{channels, TokioRuntime};
use pool_actions::traits::{HasCurrentTask, HasWorkerPeers};
use pool_messages::{PoolMsg, WorkerMsg};
use pool_messages::WorkerCtrl;
use worker_blox::{WorkerCtx, WorkerSpec};

pub mod prelude {
    pub use crate::spawn_worker_tokio;
}

/// Behavior type for Worker actors holding task state and peer list.
pub struct WorkerBehavior<R: BloxRuntime> {
    task_id: u32,
    result: u32,
    peers: Vec<ActorRef<WorkerMsg, R>>,
}

impl<R: BloxRuntime> Default for WorkerBehavior<R> {
    fn default() -> Self {
        Self {
            task_id: 0,
            result: 0,
            peers: Vec::new(),
        }
    }
}

impl<R: BloxRuntime> HasCurrentTask for WorkerBehavior<R> {
    fn task_id(&self) -> u32 {
        self.task_id
    }
    fn set_task_id(&mut self, id: u32) {
        self.task_id = id;
    }
    fn result(&self) -> u32 {
        self.result
    }
    fn set_result(&mut self, r: u32) {
        self.result = r;
    }
}

impl<R: BloxRuntime> HasWorkerPeers<R> for WorkerBehavior<R> {
    fn peers(&self) -> &[ActorRef<WorkerMsg, R>] {
        &self.peers
    }
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>> {
        &mut self.peers
    }
}

/// Spawn one worker actor and return its domain + ctrl refs.
pub fn spawn_worker_tokio(
    _pool_id: ActorId,
    pool_ref: &ActorRef<PoolMsg, TokioRuntime>,
) -> (
    ActorRef<WorkerMsg, TokioRuntime>,
    ActorRef<WorkerCtrl<TokioRuntime>, TokioRuntime>,
) {
    // Ctrl channel at index 0 (higher priority) so AddPeer commands are
    // processed before DoWork arrives on the domain channel.
    let ((ctrl_ref, domain_ref), worker_mbox) =
        channels! { WorkerCtrl<TokioRuntime>(16), WorkerMsg(16) };
    let worker_id = ctrl_ref.id();

    let behavior = WorkerBehavior::default();
    let worker_ctx = WorkerCtx::new(worker_id, pool_ref.clone(), behavior);
    let machine =
        StateMachine::<WorkerSpec<TokioRuntime, WorkerBehavior<TokioRuntime>>>::new(worker_ctx);

    TokioRuntime::spawn(async move {
        run_actor_auto_start(machine, worker_mbox).await;
    });

    (domain_ref, ctrl_ref)
}
