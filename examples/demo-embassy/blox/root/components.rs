// Copyright 2025 Bloxide, all rights reserved

use super::{ext_state::RootExtState, messaging::RootMessage, states::RootStates};
use crate::blox::counter::runtime::*;
use bloxide::{
    core::{components::*, messaging::*},
    runtime::*,
};

pub struct RootComponents;

impl Components for RootComponents {
    type States = RootStates;
    type MessageSet = RootMessage;
    type ExtendedState = RootExtState;
    type Receivers = RootReceivers;
    type Handles = RootHandles;
}

pub struct RootHandles {
    pub standard_handle: StandardMessageHandle,
    pub counter_handle: CounterHandle,
}

pub struct RootReceivers {
    pub std_rx: <StandardMessageHandle as MessageSender>::ReceiverType,
    pub counter_rx: <CounterHandle as MessageSender>::ReceiverType,
}
