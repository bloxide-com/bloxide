// Copyright 2025 Bloxide, all rights reserved

use bloxide::core::messaging::*;
use bloxide::std_exports::*;

#[derive(Debug)]
pub enum CounterMessageSet {
    StandardMessage(Message<StandardPayload>),
    CounterMessage(Message<CounterPayload>),
}
impl MessageSet for CounterMessageSet {}

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
