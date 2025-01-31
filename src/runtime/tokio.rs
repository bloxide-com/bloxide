// Copyright 2025 Bloxide, all rights reserved

#[cfg(feature = "runtime-tokio")]
pub mod runtime {
    use crate::{core::messaging::*, std_exports::*};
    pub use tokio::{pin, runtime::Builder, select, sync::mpsc, task::*, time::*};

    pub const DEFAULT_CHANNEL_SIZE: usize = 32;

    pub const STANDARD_MESSAGE_CHANNEL_SIZE: usize = DEFAULT_CHANNEL_SIZE;

    pub type TokioHandle<M> = Handle<mpsc::Sender<Message<M>>>;

    impl<M> fmt::Debug for TokioHandle<M> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "TokioHandle<{}>", self.dest_id)
        }
    }

    impl<M> MessageSender for TokioHandle<M> {
        type PayloadType = M;
        type SenderType = mpsc::Sender<Message<M>>;
        type ReceiverType = mpsc::Receiver<Message<M>>;

        fn create_channel_with_size(
            id: u16,
            size: usize,
        ) -> (Handle<Self::SenderType>, Self::ReceiverType) {
            let (sender, receiver) = mpsc::channel::<Message<M>>(size);
            let handle = Handle::new(id, sender);
            (handle, receiver)
        }

        fn try_send(&self, msg: Message<M>) -> Result<(), TrySendError<Message<M>>> {
            self.sender.try_send(msg)
        }
    }

    pub type StandardMessageHandle = TokioHandle<StandardPayload>;
}
// Re-export everything from the internal module
#[cfg(feature = "runtime-tokio")]
pub use runtime::*;
