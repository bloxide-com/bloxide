// Copyright 2025 Bloxide, all rights reserved
//! Concrete runtime-specific worker factory for the Tokio pool demo.
//!
//! This crate is the impl layer for `tokio-pool-demo`. It is the only place
//! that knows about concrete worker context/spec types and task spawning.

extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap, SpawnCap},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::ActorRef,
    StateMachine,
};
use bloxide_supervisor_context::{SpawnFactory, SpawnOutput, SpawnPolicy};
use bloxide_tokio::{run_supervised_actor, TokioRuntime};
use pool_actions::traits::{HasCurrentTask, HasWorkerPeers};
use pool_messages::{AppSpawnRequest, PoolMsg, SpawnedWorker, WorkerCtrl, WorkerMsg};
use worker_blox::{WorkerCtx, WorkerSpec};

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

/// Concrete spawn factory for the Tokio pool demo.
/// Creates worker actors and reports lifecycle events to the supervisor.
pub struct AppSpawnFactory {
    pool_ref: ActorRef<PoolMsg, TokioRuntime>,
}

impl AppSpawnFactory {
    pub fn new(pool_ref: ActorRef<PoolMsg, TokioRuntime>) -> Self {
        Self { pool_ref }
    }
}

impl SpawnFactory<TokioRuntime> for AppSpawnFactory {
    type Request = AppSpawnRequest<TokioRuntime>;

    fn spawn(
        &self,
        req: Self::Request,
        notify: ActorRef<ChildLifecycleEvent, TokioRuntime>,
    ) -> SpawnOutput<TokioRuntime> {
        match req {
            AppSpawnRequest::Worker { task_id: _, reply_to } => {
                let worker_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
                let (ctrl_ref, ctrl_rx) =
                    <TokioRuntime as DynamicChannelCap>::channel::<WorkerCtrl<TokioRuntime>>(
                        worker_id, 16,
                    );
                let (domain_ref, domain_rx) =
                    <TokioRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 16);
                let (lifecycle_ref, lifecycle_rx) =
                    <TokioRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(worker_id, 4);

                let behavior = WorkerBehavior::<TokioRuntime>::default();
                let worker_ctx = WorkerCtx::new(self.pool_ref.clone(), worker_id, behavior);
                let machine = StateMachine::<
                    WorkerSpec<TokioRuntime, WorkerBehavior<TokioRuntime>>,
                >::new(worker_ctx);

                let notify_sender = notify.sender();
                <TokioRuntime as SpawnCap>::spawn(async move {
                    run_supervised_actor(
                        machine,
                        (ctrl_rx, domain_rx),
                        lifecycle_rx,
                        worker_id,
                        notify_sender,
                    )
                    .await;
                });

                let _ = reply_to.try_send(
                    worker_id,
                    SpawnedWorker {
                        child_id: worker_id,
                        domain_ref: domain_ref.clone(),
                        ctrl_ref: ctrl_ref.clone(),
                    },
                );

                SpawnOutput {
                    child_id: worker_id,
                    lifecycle_ref,
                    policy: Some(SpawnPolicy::Stop),
                }
            }
        }
    }
}
