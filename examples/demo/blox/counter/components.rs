// Copyright 2025 Bloxide, all rights reserved

use super::{ext_state::*, messaging::*, runtime::*, states::*};
use bloxide::{
    core::{components::*, messaging::*},
    runtime::*,
};

pub struct CounterComponents;

impl Components for CounterComponents {
    type States = CounterStateEnum;
    type MessageSet = CounterMessageSet;
    type ExtendedState = CounterExtendedState;
    type Receivers = CounterReceivers;
}

pub struct CounterReceivers {
    pub standard_receiver: <StandardMessageHandle as MessageSender>::ReceiverType,
    pub counter_receiver: <CounterHandle as MessageSender>::ReceiverType,
}
