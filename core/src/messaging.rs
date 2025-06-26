// Copyright 2025 Bloxide, all rights reserved

use crate::prelude::*;
use serde::{Deserialize, Serialize};

/// Basic message type that wraps any payload and has an id
#[derive(Debug)]
pub struct Message<P> {
    pub source_id: u16,
    pub payload: P,
}

impl<P> Message<P> {
    pub fn new(source_id: u16, payload: P) -> Self {
        Self { source_id, payload }
    }

    pub fn source_id(&self) -> u16 {
        self.source_id
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RawPayload {
    pub to: u16,
    pub from: u16,
    pub payload: Vec<u8>,
}

/// Marker trait for message sets
pub trait MessageSet {}

/// Trait for handles to send messages
pub trait MessageSender {
    type PayloadType;
    type SenderType;
    type ReceiverType;
    type ErrorType: fmt::Debug;
    fn try_send(&self, msg: Message<Self::PayloadType>) -> Result<(), Self::ErrorType>;

    fn id(&self) -> u16;

    fn create_channel_with_size(id: u16, size: usize) -> (Self, Self::ReceiverType)
    where
        Self: Sized;
}

/// Enum for standard payloads
pub enum StandardPayload<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
{
    Shutdown,
    PollHandle,
    Handle(Box<dyn Any + Send>),
    PollState,
    State(Box<dyn Any + Send>),
    Error(Box<String>),
    StandardChannel(
        R::MessageHandle<StandardPayload<R>>,
        <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType,
    ),
    RawInbound(u16, Vec<u8>),
    RawOutbound(u16, Vec<u8>),
}
