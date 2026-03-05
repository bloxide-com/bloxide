use crate::channel::{EmbassySender, EmbassyStream, EmbassyTrySendError};
use crate::EmbassyRuntime;
use alloc::boxed::Box;
use bloxide_core::{
    capability::{BloxRuntime, StaticChannelCap},
    messaging::{ActorId, ActorRef, Envelope},
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

// ── BloxRuntime impl ──────────────────────────────────────────────────────────

#[allow(async_fn_in_trait)]
impl BloxRuntime for EmbassyRuntime {
    type SendError = core::convert::Infallible;
    type TrySendError = EmbassyTrySendError;
    type Sender<M: Send + 'static> = EmbassySender<M>;
    type Receiver<M: Send + 'static> = EmbassyStream<M>;
    type Stream<M: Send + 'static> = EmbassyStream<M>;

    fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M> {
        rx
    }

    async fn send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::SendError> {
        sender.inner.send(envelope).await;
        Ok(())
    }

    fn try_send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::TrySendError> {
        sender
            .inner
            .try_send(envelope)
            .map_err(|_| EmbassyTrySendError)
    }
}

// ── StaticChannelCap impl ─────────────────────────────────────────────────────

impl StaticChannelCap for EmbassyRuntime {
    fn channel<M: Send + 'static, const N: usize>(
        id: ActorId,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
        // Leak a heap-allocated Channel so we get a 'static reference.
        // Embassy actors are process-lifetime objects — this is intentional.
        //
        // CriticalSectionRawMutex is the single configuration point for the
        // channel mutex type. Swap for another RawMutex impl if targeting
        // multi-core (e.g., RP2350 SMP) or other synchronization strategies.
        let ch: &'static Channel<CriticalSectionRawMutex, Envelope<M>, N> =
            Box::leak(Box::new(Channel::new()));
        let sender = EmbassySender {
            inner: ch.dyn_sender(),
        };
        let stream = EmbassyStream {
            inner: ch.dyn_receiver(),
        };
        (ActorRef::new(id, sender), stream)
    }
}
