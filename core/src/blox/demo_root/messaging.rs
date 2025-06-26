// Copyright 2025 Bloxide, all rights reserved

use crate::{blox::demo_counter::messaging::CounterPayload, prelude::*};

pub enum RootMessageSet<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
{
    StandardMessage(Message<StandardPayload<R>>),
    CounterMessage(Message<CounterPayload>),
}

impl<R: Runtime> MessageSet for RootMessageSet<R> where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send
{
}
