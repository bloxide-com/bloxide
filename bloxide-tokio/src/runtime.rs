// Copyright 2025 Bloxide, all rights reserved

use bloxide_core::prelude::*;
pub use tokio::{pin, select, sync::mpsc, sync::mpsc::error::TrySendError, time::*};

pub const DEFAULT_CHANNEL_SIZE: usize = 32;

pub const STANDARD_MESSAGE_CHANNEL_SIZE: usize = DEFAULT_CHANNEL_SIZE;

use bloxide_core::components::Runtime;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Clone)]
pub struct TokioRuntime;

impl Runtime for TokioRuntime {
    type MessageHandle<P: Send + 'static> = TokioMessageHandle<P>;

    fn spawn<F>(f: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        tokio::spawn(f);
    }

    type ReceiverStream<P: Send + 'static> = ReceiverStream<Message<P>>;

    fn to_stream<P: Send + 'static>(
        receiver: <Self::MessageHandle<P> as MessageSender>::ReceiverType,
    ) -> Self::ReceiverStream<P> {
        ReceiverStream::new(receiver)
    }
}

#[derive(Debug)]
pub struct TokioMessageHandle<P: Send + 'static> {
    id: u16,
    sender: mpsc::Sender<Message<P>>,
}

impl<P: Send + 'static> Clone for TokioMessageHandle<P> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            sender: self.sender.clone(),
        }
    }
}

impl<P: Send + 'static> MessageSender for TokioMessageHandle<P> {
    type PayloadType = P;
    type SenderType = mpsc::Sender<Message<P>>;
    type ReceiverType = mpsc::Receiver<Message<P>>;
    type ErrorType = mpsc::error::TrySendError<Message<P>>;

    fn try_send(&self, msg: Message<P>) -> Result<(), Self::ErrorType> {
        self.sender.try_send(msg)
    }

    fn id(&self) -> u16 {
        self.id
    }

    fn create_channel_with_size(id: u16, size: usize) -> (Self, Self::ReceiverType) {
        let (tx, rx) = mpsc::channel(size);
        (Self { id, sender: tx }, rx)
    }
}
