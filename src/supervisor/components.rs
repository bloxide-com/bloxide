// Copyright 2025 Bloxide, all rights reserved

use super::{ext_state::*, messaging::*, runtime::*, states::*};
use crate::{core::components::*, core::messaging::*, runtime::*, std_exports::*};
use embassy_executor::SendSpawner;
pub struct SupervisorComponents;

impl Components for SupervisorComponents {
    type States = SupervisorStateEnum;
    type MessageSet = SupervisorMessageSet;
    type ExtendedState = SupervisorExtendedState;
    type Receivers = SupervisorReceivers;
    type Handles = SupervisorHandles<StandardMessageHandle, SupervisorHandle>;
}

pub struct SupervisorReceivers {
    pub standard_receiver: <StandardMessageHandle as MessageSender>::ReceiverType,
    pub supervisor_receiver: <SupervisorHandle as MessageSender>::ReceiverType,
}

/* pub struct SupervisorHandles {
    pub standard_handle: impl MessageSender<StandardPayload>,
    pub supervisor_handle: impl MessageSender<SupervisorPayload>,
} */

pub struct SupervisorHandles<H1, H2> 
where
    H1: MessageSender,
    H2: MessageSender,
{
    pub standard_handle: H1,
    pub supervisor_handle: H2,
}

pub struct SupervisorInitArgs {
    pub root_standard_handle: StandardMessageHandle,
    pub spawner: Option<SendSpawner>,
    pub root_spawn_fn: Option<RootSpawnFn>,
}

pub(super) type RootSpawnFn = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;
