// Copyright 2025 Bloxide, all rights reserved
use alloc::boxed::Box;

use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    messaging::{ActorId, ActorRef, Envelope},
};

use crate::{
    channel::{WasmSendError, WasmSender, WasmStream, WasmTrySendError},
    WasmRuntime,
};

static NEXT_WASM_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

fn alloc_wasm_id() -> ActorId {
    NEXT_WASM_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl BloxRuntime for WasmRuntime {
    type SendError = WasmSendError;
    type TrySendError = WasmTrySendError;
    type Sender<M: Send + 'static> = WasmSender<M>;
    type Receiver<M: Send + 'static> = WasmStream<M>;
    type Stream<M: Send + 'static> = WasmStream<M>;

    fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M> {
        rx
    }

    async fn send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::SendError> {
        sender
            .inner
            .send(envelope)
            .await
            .map_err(|_| WasmSendError)
    }

    fn try_send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::TrySendError> {
        sender
            .inner
            .try_send(envelope)
            .map_err(|_| WasmTrySendError)
    }
}

impl DynamicChannelCap for WasmRuntime {
    fn alloc_actor_id() -> ActorId {
        alloc_wasm_id()
    }

    fn channel<M: Send + 'static>(
        id: ActorId,
        capacity: usize,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
        let (tx, rx) = async_channel::bounded::<Envelope<M>>(capacity);
        let sender = WasmSender { inner: tx };
        let stream = WasmStream {
            inner: Box::pin(rx),
        };
        (ActorRef::new(id, sender), stream)
    }
}
