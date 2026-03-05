//! Unified event type for the Worker actor.
//!
//! The `Mailboxes` tuple is:
//!   `(R::Stream<PeerCtrl<WorkerMsg, R>>, R::Stream<WorkerMsg>)`
//!   index 0 = Ctrl (higher priority — ensures AddPeer runs before DoWork)
//!   index 1 = Msg  (domain messages)
use bloxide_core::{capability::BloxRuntime, messaging::Envelope};
use bloxide_macros::blox_event;
use bloxide_spawn::peer::PeerCtrl;
use pool_messages::WorkerMsg;

/// Combined event type for the Worker actor.
///
/// `#[blox_event]` generates:
/// - `From<Envelope<PeerCtrl<WorkerMsg, R>>>` for `WorkerEvent<R>` (index 0)
/// - `From<Envelope<WorkerMsg>>` for `WorkerEvent<R>` (index 1)
/// - `EventTag` impl: Ctrl → 0, Msg → 1
/// - Payload accessors: `ctrl_payload()`, `msg_payload()`
///
/// Note: `Debug` is intentionally not derived — `PeerCtrl<M, R>` does not
/// implement `Debug`, so the full enum cannot either.
#[blox_event]
pub enum WorkerEvent<R: BloxRuntime> {
    /// Peer-control command (AddPeer / RemovePeer). Polled first (higher priority).
    Ctrl(Envelope<PeerCtrl<WorkerMsg, R>>),
    /// Domain message (DoWork / PeerResult).
    Msg(Envelope<WorkerMsg>),
}
