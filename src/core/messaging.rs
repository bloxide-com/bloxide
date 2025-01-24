// Copyright 2025 Bloxide, all rights reserved

use crate::std_exports::*;

/// Basic message type that wraps any payload and has an id. Usually used as a "source id"
#[derive(Debug)]
pub struct Message<P> {
    pub id: u16,
    pub payload: P,
}

impl<P> Message<P> {
    pub fn new(id: u16, payload: P) -> Self {
        Self { id, payload }
    }

    pub fn id(&self) -> u16 {
        self.id
    }
}

/// Handle type that corresponds to a specific message type
#[derive(Debug)]
pub struct Handle<P, S> {
    pub id: u16,
    pub sender: S,
    pub _phantom: PhantomData<P>,
}

impl<P, S> Handle<P, S> {
    pub fn new(id: u16, sender: S) -> Self {
        Self {
            id,
            sender,
            _phantom: PhantomData,
        }
    }

    pub fn id(&self) -> u16 {
        self.id
    }
}

/// Add manual Clone implementation that only requires S: Clone
/// This allows `Message<T>`to be passed by value if necessary
impl<P, S: Clone> Clone for Handle<P, S> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            sender: self.sender.clone(),
            _phantom: PhantomData,
        }
    }
}

/// Implement type erasure to send any Handle over channels
impl<P: 'static, S: Clone + 'static> Handle<P, S> {
    pub fn into_erased(self) -> Box<dyn Any> {
        Box::new(self)
    }
}

/// Trait for handles to send messages
pub trait MessageSender {
    type PayloadType;
    fn try_send(
        &self,
        msg: Message<Self::PayloadType>,
    ) -> Result<(), TrySendError<Message<Self::PayloadType>>>;
}

/// Enum for standard payloads
pub enum StandardPayload {
    Initialize,
    Shutdown,
    RegisterSender(Box<dyn Any + Send>),
    SpawnRequest(
        Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> + Send + 'static>,
    ),
    //TODO: Make seperate Send and !Send Payload versions
    //SpawnLocalRequest(Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + 'static>>>),
    Error(Box<String>),
}

impl Debug for StandardPayload {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            StandardPayload::Initialize => write!(f, "Initialize"),
            StandardPayload::Shutdown => write!(f, "Shutdown"),
            StandardPayload::RegisterSender(_) => write!(f, "RegisterSender(...)"),
            StandardPayload::SpawnRequest(_) => write!(f, "SpawnRequest(...)"),
            //StandardPayload::SpawnLocalRequest(_) => write!(f, "SpawnLocalRequest(...)"),
            StandardPayload::Error(msg) => write!(f, "Error({})", msg),
        }
    }
}
