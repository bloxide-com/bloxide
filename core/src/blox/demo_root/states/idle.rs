// Copyright 2025 Bloxide, all rights reserved

use super::uninit::Uninit;
use super::{RootComponents, RootStates};
use crate::{blox::demo_counter::messaging::CounterPayload, components::Runtime, prelude::*};

#[derive(Clone, PartialEq, Debug)]
pub struct Idle;

impl<R: Runtime> State<RootComponents<R>> for Idle
where
    R::MessageHandle<StandardPayload<R>>: Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    R::MessageHandle<CounterPayload>: Clone + Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send,
{
    fn parent(&self) -> RootStates {
        RootStates::Uninit(Uninit)
    }

    fn handle_message(
        &self,
        _state_machine: &mut StateMachine<RootComponents<R>>,
        _msg: <RootComponents<R> as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents<R> as Components>::MessageSet>> {
        None
    }
}
