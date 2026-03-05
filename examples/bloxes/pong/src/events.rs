use bloxide_core::messaging::Envelope;
use bloxide_macros::blox_event;
use ping_pong_messages::PingPongMsg;

/// Unified event type for the Pong actor.
///
/// The `Mailboxes` tuple is `(R::Stream<PingPongMsg>,)`.
/// Pong only handles `PingPongMsg::Ping` variants; others are silently dropped.
#[blox_event]
#[derive(Debug)]
pub enum PongEvent {
    /// Domain message from a peer.
    Msg(Envelope<PingPongMsg>),
}
