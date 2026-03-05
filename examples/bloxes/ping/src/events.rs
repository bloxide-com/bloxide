use bloxide_core::messaging::Envelope;
use bloxide_macros::blox_event;
use ping_pong_messages::PingPongMsg;

/// Unified event type for the Ping actor.
///
/// The `Mailboxes` tuple is `(R::Stream<PingPongMsg>,)`,
/// so the `From` impl matches index 0.
#[blox_event]
#[derive(Debug)]
pub enum PingEvent {
    /// Domain message from a peer.
    Msg(Envelope<PingPongMsg>),
}
