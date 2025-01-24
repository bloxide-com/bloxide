//Copyright 2025 Bloxide, all rights reserved

#[cfg(feature = "runtime-embassy")]
pub mod runtime {
    use crate::core::messaging::*;
    use crate::std_exports::*;
    use core::cell::RefCell;
    use embassy_sync::blocking_mutex::raw::RawMutex;
    use embassy_sync::channel::Channel;
    use embassy_sync::channel::TrySendError;

    pub type DefaultChannelMutex = embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    pub const DEFAULT_CHANNEL_SIZE: usize = 8;

    pub type StandardMessageChannelMutex = DefaultChannelMutex;
    pub const STANDARD_MESSAGE_CHANNEL_SIZE: usize = DEFAULT_CHANNEL_SIZE;
    pub type StandardMessageHandle =
        EmbassyHandle<StandardPayload, StandardMessageChannelMutex, STANDARD_MESSAGE_CHANNEL_SIZE>;
    pub type StandardMessagePool = ChannelPool<StandardMessageHandle>;

    /// An entry in our pool: a single channel plus "in_use" flag
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
        M: Send,
    {
        /// Creates a pool of `capacity` distinct channels, each leaked to `'static`.
        pub fn new(capacity: u16) -> Self {
            let vec = (0..capacity)
                .map(|i| {
                    let channel: &'static Channel<Mutex, Message<M>, Q> =
                        Box::leak(Box::new(Channel::new()));
                    PooledHandle {
                        handle: Some(Handle::new(i, channel)),
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
        pub fn release(&self, handle: EmbassyHandle<M, Mutex, Q>)
        where
            M: MessageSender,
        {
            let mut guard = self.pool.borrow_mut();
            if let Some(entry) = guard.iter_mut().find(|entry| entry.handle.is_none()) {
                entry.handle = Some(handle);
            }
        }
    }

    /// A convenience type alias
    pub type EmbassyHandle<M, Mutex, const Q: usize> =
        Handle<M, &'static Channel<Mutex, Message<M>, Q>>;

    impl<M, Mutex: RawMutex + 'static, const Q: usize> MessageSender for EmbassyHandle<M, Mutex, Q> {
        type PayloadType = M;
        fn try_send(&self, message: Message<M>) -> Result<(), TrySendError<Message<M>>> {
            self.sender.sender().try_send(message)
        }
    }
}

#[cfg(feature = "runtime-embassy")]
pub use runtime::*;
