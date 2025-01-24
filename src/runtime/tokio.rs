//Copyright 2025 Bloxide, all rights reserved

#[cfg(feature = "runtime-tokio")]
pub mod runtime {
    use crate::core::{actor::*, messaging::*};
    use crate::std_exports::*;
    use tokio::sync::mpsc;

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

    #[derive(Debug)]
    pub struct TokioActorSpawner<const Q: usize, A: Actor> {
        phantom: PhantomData<A>,
    }

    impl<const Q: usize, A: Actor> Default for TokioActorSpawner<Q, A> {
        fn default() -> Self {
            Self {
                phantom: PhantomData,
            }
        }
    }

    impl<const Q: usize, A: Actor> TokioActorSpawner<Q, A> {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl<const Q: usize, A> ActorSpawner for TokioActorSpawner<Q, A>
    where
        A: Actor + Debug + 'static,
        A::MessageSet: From<Message<StandardPayload>>,
        A::InitArgs: Default,
    {
        fn spawn(&self, id: u16) -> Result<StandardMessageHandle, ActorError> {
            let (tx, mut rx) =
                mpsc::channel::<Message<StandardPayload>>(STANDARD_MESSAGE_CHANNEL_SIZE);
            let handle = StandardMessageHandle::new(id, tx);
            let mut actor = A::new(id, handle.clone(), Default::default());

            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    actor.handle_message(&msg.into());
                }
            });

            Ok(handle)
        }
    }
}

// Re-export everything from the internal module
#[cfg(feature = "runtime-tokio")]
pub use runtime::*;
