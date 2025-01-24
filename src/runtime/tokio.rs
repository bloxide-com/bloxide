//Copyright 2025 Bloxide, all rights reserved

#[cfg(feature = "runtime-tokio")]
pub mod runtime {
    use crate::core::messaging::*;
    use crate::std_exports::*;
    pub use tokio::runtime::Builder;
    pub use tokio::select;
    pub use tokio::spawn;
    pub use tokio::sync::mpsc;
    pub use tokio::time::{sleep, Duration};

    pub const DEFAULT_CHANNEL_SIZE: usize = 32;

    pub const STANDARD_MESSAGE_CHANNEL_SIZE: usize = DEFAULT_CHANNEL_SIZE;

    pub type TokioHandle<M> = Handle<M, mpsc::Sender<Message<M>>>;

    impl<M> MessageSender for TokioHandle<M> {
        type PayloadType = M;

        fn try_send(&self, msg: Message<M>) -> Result<(), TrySendError<Message<M>>> {
            self.sender.try_send(msg)
        }
    }

    pub type StandardMessageHandle = TokioHandle<StandardPayload>;
}
// Re-export everything from the internal module
#[cfg(feature = "runtime-tokio")]
pub use runtime::*;
