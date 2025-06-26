// Copyright 2025 Bloxide, all rights reserved

use crate::prelude::*;

use super::SupervisorInitArgs;

#[derive(Default)]
pub struct SupervisorExtendedState<R: Runtime>
where
    R::MessageHandle<StandardPayload<R>>:
        MessageSender<PayloadType = StandardPayload<R>> + Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
{
    pub blox: HashMap<u16, R::MessageHandle<StandardPayload<R>>>,
    pub next_id: u16,
    pub root_future: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

#[allow(clippy::type_complexity)]
impl<R: Runtime> SupervisorExtendedState<R>
where
    R::MessageHandle<StandardPayload<R>>:
        MessageSender<PayloadType = StandardPayload<R>> + Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
{
    pub fn request_new_standard_handle(
        &mut self,
        queue_size: usize,
    ) -> (
        R::MessageHandle<StandardPayload<R>>,
        <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType,
    ) {
        let (handle, rx) = R::MessageHandle::<StandardPayload<R>>::create_channel_with_size(
            self.next_id,
            queue_size,
        );
        self.blox.insert(self.next_id, handle.clone());
        self.next_id += 1;
        (handle, rx)
    }

    pub fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Result<(), String> {
        R::spawn(future);
        // Add implementation here
        Ok(())
    }
}

impl<R: Runtime> ExtendedState for SupervisorExtendedState<R>
where
    R::MessageHandle<StandardPayload<R>>:
        MessageSender<PayloadType = StandardPayload<R>> + Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
{
    type InitArgs = SupervisorInitArgs<R>;

    fn new(args: Self::InitArgs) -> Self {
        let mut blox = HashMap::new();
        blox.insert(
            args.root_standard_handle.id(),
            args.root_standard_handle.clone(),
        );
        SupervisorExtendedState {
            blox,
            root_future: Some(args.root_future),
            next_id: 2,
        }
    }
}

impl<R: Runtime> fmt::Debug for SupervisorExtendedState<R>
where
    R::MessageHandle<StandardPayload<R>>: Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SupervisorExtendedState")
    }
}
