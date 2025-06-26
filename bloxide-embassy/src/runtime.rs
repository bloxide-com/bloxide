// Copyright 2025 Bloxide, all rights reserved

use bloxide_core::prelude::*;
use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, RawMutex};
use embassy_sync::channel::Channel;

pub const DEFAULT_CHANNEL_SIZE: usize = 8;
pub const STANDARD_MESSAGE_CHANNEL_SIZE: usize = DEFAULT_CHANNEL_SIZE;

pub type DefaultChannelMutex = CriticalSectionRawMutex;
pub type StandardMessageChannelMutex = DefaultChannelMutex;
pub type StandardMessageHandle<R> =
    EmbassyHandle<StandardPayload<R>, StandardMessageChannelMutex, STANDARD_MESSAGE_CHANNEL_SIZE>;
pub type StandardMessagePool<R> = ChannelPool<StandardMessageHandle<R>>;

pub struct EmbassyReceiver<M: 'static, Mutex: RawMutex + Sync + 'static, const Q: usize> {
    pub channel: &'static Channel<Mutex, Message<M>, Q>,
}

#[derive(Clone)]
pub struct EmbassyHandle<M: 'static, Mutex: RawMutex + Sync + 'static, const Q: usize> {
    pub id: u16,
    pub channel: &'static Channel<Mutex, Message<M>, Q>,
}

impl<M, Mutex: RawMutex + Sync + 'static, const Q: usize> EmbassyHandle<M, Mutex, Q> {
    pub fn new(id: u16, channel: &'static Channel<Mutex, Message<M>, Q>) -> Self {
        Self { id, channel }
    }
}

impl<M: 'static, Mutex: RawMutex + Sync + 'static, const Q: usize> fmt::Debug
    for EmbassyReceiver<M, Mutex, Q>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EmbassyReceiver")
    }
}

impl<M: 'static, Mutex: RawMutex + Sync + 'static, const Q: usize> fmt::Debug
    for EmbassyHandle<M, Mutex, Q>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EmbassySender, ID: {}", self.id)
    }
}
struct PooledHandle<H> {
    handle: Option<H>,
}

pub struct ChannelPool<H>
where
    H: Clone + Send,
{
    pool: RefCell<Vec<PooledHandle<H>>>,
}

impl<M: 'static, Mutex: RawMutex + Sync + 'static, const Q: usize>
    ChannelPool<EmbassyHandle<M, Mutex, Q>>
where
    M: Send + Clone,
    Mutex: Clone,
{
    /// Creates a pool of `capacity` distinct channels, each leaked to `'static`.
    pub fn new(capacity: u16) -> Self {
        let vec = (0..capacity)
            .map(|i| {
                let channel: &'static Channel<Mutex, Message<M>, Q> =
                    Box::leak(Box::new(Channel::new()));
                PooledHandle {
                    handle: Some(EmbassyHandle { id: i, channel }),
                }
            })
            .collect();

        Self {
            pool: RefCell::new(vec),
        }
    }

    /// Acquire an unused channel from the pool
    pub fn acquire(&self) -> Option<EmbassyHandle<M, Mutex, Q>> {
        let mut guard = self.pool.borrow_mut();
        guard.iter_mut().find_map(|entry| entry.handle.take())
    }

    /// Release (return) the channel to the pool
    pub fn release(&self, handle: EmbassyHandle<M, Mutex, Q>) {
        let mut guard = self.pool.borrow_mut();
        if let Some(entry) = guard.iter_mut().find(|entry| entry.handle.is_none()) {
            entry.handle = Some(handle);
        }
    }
}

impl<M, Mutex: RawMutex + Sync + 'static, const Q: usize> MessageSender
    for EmbassyHandle<M, Mutex, Q>
{
    type PayloadType = M;
    type SenderType = &'static Channel<Mutex, Message<M>, Q>;
    type ReceiverType = EmbassyReceiver<M, Mutex, Q>;
    type ErrorType = ();
    fn try_send(&self, message: Message<M>) -> Result<(), Self::ErrorType> {
        self.channel.try_send(message).map_err(|_| ())
    }
    fn create_channel_with_size(
        id: u16,
        _size: usize,
    ) -> (EmbassyHandle<M, Mutex, Q>, Self::ReceiverType) {
        let channel: &'static Channel<Mutex, Message<M>, Q> = Box::leak(Box::new(Channel::new()));
        (EmbassyHandle { id, channel }, EmbassyReceiver { channel })
    }

    fn id(&self) -> u16 {
        self.id
    }
}
