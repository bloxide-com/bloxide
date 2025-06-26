// Copyright 2025 Bloxide, all rights reserved

use super::idle::Idle;
use super::{RootComponents, RootStates};
use crate::{blox::demo_counter::messaging::CounterPayload, components::Runtime, prelude::*};
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Finished;

impl<R: Runtime> State<RootComponents<R>> for Finished
where
    R::MessageHandle<StandardPayload<R>>: Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    R::MessageHandle<CounterPayload>: Clone + Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send,
{
    fn parent(&self) -> RootStates {
        RootStates::Idle(Idle)
    }

    fn handle_message(
        &self,
        _state_machine: &mut StateMachine<RootComponents<R>>,
        _msg: <RootComponents<R> as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents<R> as Components>::MessageSet>> {
        //Program is finished, no more messages
        None
    }

    fn on_entry(&self, _data: &mut StateMachine<RootComponents<R>>) {
        trace!("State on_entry: {:?}", self);
        info!("♫ I CAN ONLY COUNT TO 4 ♫");
    }
}
