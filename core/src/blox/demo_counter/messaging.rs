// Copyright 2025 Bloxide, all rights reserved

use crate::components::Runtime;
use crate::{messaging::*, prelude::*};
pub enum CounterMessageSet<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    StandardMessage(Message<StandardPayload<R>>),
    CounterMessage(Message<CounterPayload>),
}
impl<R: Runtime> MessageSet for CounterMessageSet<R>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
}

#[derive(Debug)]
pub enum CounterPayload {
    SetCount(Box<usize>),
    Increment(Box<usize>),
    Decrement(Box<usize>),
    SetMax(Box<usize>),
    SetMin(Box<usize>),
    CountEvent(Box<CountEvent>),
}

#[derive(Debug)]
pub enum CountEvent {
    GetCount,
    MaxReached,
    MinReached,
    Reset,
    StartCounting,
}
