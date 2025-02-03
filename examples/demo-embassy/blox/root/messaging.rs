// Copyright 2025 Bloxide, all rights reserved

use bloxide::{Message, MessageSet, StandardPayload};

use crate::blox::counter::messaging::CounterPayload;

#[derive(Debug)]
pub enum RootMessage {
    StandardMessage(Message<StandardPayload>),
    CounterMessage(Message<CounterPayload>),
}

impl MessageSet for RootMessage {}
