extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{
    accessor::HasSelfId,
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
};

/// Control message for peer introduction between actors of the same message type.
///
/// This is a framework type from `bloxide-spawn`. Blox crates never define their
/// own control message types — use `PeerCtrl<M, R>` directly.
///
/// `PeerCtrl` channels are polled alongside domain mailboxes in the actor's
/// `Mailboxes` tuple — one recv loop, one dispatch call, same as always.
pub enum PeerCtrl<M: Send + 'static, R: BloxRuntime> {
    AddPeer(AddPeer<M, R>),
    RemovePeer(RemovePeer),
}

pub struct AddPeer<M: Send + 'static, R: BloxRuntime> {
    pub peer_ref: ActorRef<M, R>,
}

pub struct RemovePeer {
    pub peer_id: ActorId,
}

/// Accessor trait for contexts that hold a collection of peer `ActorRef`s.
///
/// Implement via `#[provides(HasPeers<WorkerMsg, R>)]` on the context field.
pub trait HasPeers<M: Send + 'static, R: BloxRuntime> {
    fn peers(&self) -> &[ActorRef<M, R>];
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<M, R>>;
}

/// Apply a `PeerCtrl` command to the context's peer collection.
///
/// Call this from a thin blox-local action wrapper that extracts the ctrl payload:
/// ```ignore
/// fn handle_ctrl(ctx: &mut WorkerCtx<R>, ev: &WorkerEvent<R>) -> ActionResult {
///     if let Some(ctrl) = ev.ctrl_payload() {
///         apply_peer_ctrl(ctx, ctrl);
///     }
///     ActionResult::Ok
/// }
/// ```
pub fn apply_peer_ctrl<M, R, C>(ctx: &mut C, ctrl: &PeerCtrl<M, R>)
where
    M: Send + 'static,
    R: BloxRuntime,
    C: HasPeers<M, R>,
{
    match ctrl {
        PeerCtrl::AddPeer(AddPeer { peer_ref }) => ctx.peers_mut().push(peer_ref.clone()),
        PeerCtrl::RemovePeer(RemovePeer { peer_id }) => {
            ctx.peers_mut().retain(|r| r.id() != *peer_id)
        }
    }
}

/// Introduce two actors as peers by sending `AddPeer` control commands.
///
/// The parent calls this after spawning both actors to wire them together.
pub fn introduce_peers<M, R, C>(
    ctx: &C,
    a_ctrl: &ActorRef<PeerCtrl<M, R>, R>,
    a_domain: &ActorRef<M, R>,
    b_ctrl: &ActorRef<PeerCtrl<M, R>, R>,
    b_domain: &ActorRef<M, R>,
) where
    M: Send + 'static,
    R: BloxRuntime,
    C: HasSelfId,
{
    let from = ctx.self_id();
    let _ = a_ctrl.try_send(
        from,
        PeerCtrl::AddPeer(AddPeer {
            peer_ref: b_domain.clone(),
        }),
    );
    let _ = b_ctrl.try_send(
        from,
        PeerCtrl::AddPeer(AddPeer {
            peer_ref: a_domain.clone(),
        }),
    );
}
