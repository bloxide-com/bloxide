// Copyright 2025 Bloxide, all rights reserved

use super::{RootComponents, RootStates};
use crate::{blox::demo_counter::messaging::CounterPayload, prelude::*};

#[derive(Clone, PartialEq, Debug)]
pub struct Uninit;

impl<R: Runtime> State<RootComponents<R>> for Uninit
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
