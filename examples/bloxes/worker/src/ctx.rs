//! Worker context — holds runtime refs and task state.
extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
};
use bloxide_macros::BloxCtx;
use bloxide_spawn::peer::HasPeers;
use pool_actions::traits::{HasCurrentTask, HasPoolRef};
use pool_messages::{PoolMsg, WorkerMsg};

/// Context for the Worker actor.
///
/// Generic over `R` (the runtime) — the blox crate never imports any concrete
/// runtime. The wiring layer injects the runtime at construction time.
///
/// `#[derive(BloxCtx)]` generates:
/// - `impl HasSelfId for WorkerCtx<R>`
/// - `impl HasPoolRef<R> for WorkerCtx<R>` via `fn pool_ref(&self) -> &ActorRef<PoolMsg, R>`
/// - `fn new(self_id, pool_ref) -> Self` (unannotated fields default to `Default::default()`)
#[derive(BloxCtx)]
pub struct WorkerCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPoolRef<R>)]
    pub pool_ref: ActorRef<PoolMsg, R>,
    /// Current task identifier. Set when DoWork arrives.
    pub task_id: u32,
    /// Computed result. Set before transitioning to Done.
    pub result: u32,
    /// Peers collected via `PeerCtrl::AddPeer`. `Vec<T>: Default` (empty vec).
    pub peers: Vec<ActorRef<WorkerMsg, R>>,
}

impl<R: BloxRuntime> HasPeers<WorkerMsg, R> for WorkerCtx<R> {
    fn peers(&self) -> &[ActorRef<WorkerMsg, R>] {
        &self.peers
    }
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>> {
        &mut self.peers
    }
}

impl<R: BloxRuntime> HasCurrentTask for WorkerCtx<R> {
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
