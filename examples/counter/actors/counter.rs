// Copyright 2025 Bloxide, all rights reserved

use bloxide::core::actor::*;
use bloxide::core::messaging::*;
use bloxide::runtime::*;
use bloxide::std_exports::*;
use log::*;

pub type CounterHandle = TokioHandle<CounterPayload>;

pub struct CounterActorConfig {
    pub standard_receiver: mpsc::Receiver<Message<StandardPayload>>,
    pub counter_receiver: mpsc::Receiver<Message<CounterPayload>>,
    pub counter_handle: CounterHandle,
}

pub struct CounterComponents {}

impl Components for CounterComponents {
    type StateEnum = CounterStateEnum;
    type MessageSet = CounterMessageSet;
    type ExtendedState = CounterExtendedState;
    type ActorConfig = CounterActorConfig;
}

impl StateEnum for CounterStateEnum {}

#[derive(Debug)]
pub enum CounterMessageSet {
    StandardMessage(Message<StandardPayload>),
    CounterMessage(Message<CounterPayload>),
}
impl MessageSet for CounterMessageSet {}

mod state {
    #[derive(Clone, PartialEq, Debug)]
    pub struct Uninit {}

    #[derive(Clone, PartialEq, Debug)]
    pub struct Idle {}

    #[derive(Clone, PartialEq, Debug)]
    pub struct NotStarted {}

    #[derive(Clone, PartialEq, Debug)]
    pub struct Counting {}

    #[derive(Clone, PartialEq, Debug)]
    pub struct Finished {}

    #[derive(Clone, PartialEq, Debug)]
    pub struct Error {}
}

use state::*; // Import the state structs for internal use

#[derive(Clone, PartialEq, Debug)]
pub enum CounterStateEnum {
    Uninit(Uninit),
    Idle(Idle),
    NotStarted(NotStarted),
    Counting(Counting),
    Finished(Finished),
    Error(Error),
}
impl Default for CounterStateEnum {
    fn default() -> Self {
        CounterStateEnum::Uninit(Uninit {})
    }
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

pub struct CounterExtendedState {
    count: usize,
    max: usize,
    min: usize,
}
impl ExtendedState for CounterExtendedState {
    fn new() -> Self {
        CounterExtendedState {
            count: 0,
            max: 0,
            min: 0,
        }
    }
}

impl Runnable<CounterComponents> for Actor<CounterComponents> {
    fn run_actor(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let mut this = self;
            trace!("Run actor started. Listening for messages...");

            loop {
                select! {
                    Some(message) = this.config.standard_receiver.recv() => {
                        trace!("Actor received STANDARD message: {:?}", message);
                        let current_state = this.state_machine.current_state.clone();
                        this.state_machine.dispatch(&CounterMessageSet::StandardMessage(message), &current_state);
                    },
                    Some(message) = this.config.counter_receiver.recv() => {
                        trace!("Actor received COUNTER message: {:?}", message);
                        let current_state = this.state_machine.current_state.clone();
                        this.state_machine.dispatch(&CounterMessageSet::CounterMessage(message), &current_state);
                    },
                    else => {
                        // If all channels closed, break out
                        trace!("All channels closed. Stopping actor run loop.");
                        break;
                    }
                }
            }
        })
    }
}

impl State<CounterComponents> for Uninit {
    fn handle_message(
        &self,
        message: &CounterMessageSet,
        _data: &mut CounterExtendedState,
    ) -> Option<Transition<CounterStateEnum>> {
        trace!("Uninit handle message");
        match message {
            CounterMessageSet::StandardMessage(message) => {
                match message.payload {
                    StandardPayload::Initialize => {
                        self.on_exit(_data); //TODO: Bug- Shoud not have to run this manually on transition
                        Some(Transition::To(CounterStateEnum::NotStarted(NotStarted {})))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
    fn on_entry(&self, _data: &mut CounterExtendedState) {
        info!("Uninit on_entry");
        info!("This is the Actor Shutdown");
    }
    fn on_exit(&self, _data: &mut CounterExtendedState) {
        info!("Uninit on_exit");
        info!("This is the Actor Initialization");
    }
    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Uninit(Uninit {})
    }
}

impl State<CounterComponents> for Idle {
    fn handle_message(
        &self,
        message: &CounterMessageSet,
        _data: &mut CounterExtendedState,
    ) -> Option<Transition<CounterStateEnum>> {
        trace!("[Idle] handle_message: {:?}", message);
        None
    }

    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Uninit(Uninit {})
    }
}

impl NotStarted {
    fn handle_counter_msg(
        &self,
        msg: &Message<CounterPayload>,
        data: &mut CounterExtendedState,
    ) -> Option<Transition<CounterStateEnum>> {
        match &msg.payload {
            CounterPayload::SetCount(new_value) => {
                data.count = **new_value;
                debug!("[Idle] Set count to {}", data.count);
                None // remain in Idle
            }
            CounterPayload::SetMax(new_max) => {
                data.max = **new_max;
                debug!("[Idle] New max set to {}", data.max);
                None
            }
            CounterPayload::SetMin(new_min) => {
                data.min = **new_min;
                debug!("[Idle] New min set to {}", data.min);
                None
            }
            CounterPayload::CountEvent(event) => {
                trace!("[Idle] Received CountEvent: {:?}", event);
                match **event {
                    CountEvent::GetCount => {
                        debug!("[Idle] Current count: {}", data.count);
                        //TODO: send count
                        None
                    }
                    CountEvent::StartCounting => {
                        Some(Transition::To(CounterStateEnum::Counting(Counting {})))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

impl State<CounterComponents> for NotStarted {
    fn handle_message(
        &self,
        message: &CounterMessageSet,
        data: &mut CounterExtendedState,
    ) -> Option<Transition<CounterStateEnum>> {
        trace!("[Idle] handle_message: {:?}", message);
        match message {
            CounterMessageSet::CounterMessage(msg) => self.handle_counter_msg(msg, data),
            _ => None,
        }
    }

    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Idle(Idle {})
    }
}

impl Counting {
    fn do_increment(&self, amount: usize, data: &mut CounterExtendedState) {
        data.count += amount;
        debug!("[Counting] Incremented by {} to {}", amount, data.count);
    }

    fn do_decrement(&self, amount: usize, data: &mut CounterExtendedState) {
        data.count -= amount;
        debug!("[Counting] Decremented by {} to {}", amount, data.count);
    }
}

impl State<CounterComponents> for Counting {
    fn handle_message(
        &self,
        message: &CounterMessageSet,
        data: &mut CounterExtendedState,
    ) -> Option<Transition<CounterStateEnum>> {
        trace!("[Counting] handle_message: {:?}", message);

        match message {
            CounterMessageSet::CounterMessage(msg) => match &msg.payload {
                CounterPayload::Increment(amount) => {
                    self.do_increment(**amount, data);
                    if data.count >= data.max {
                        Some(Transition::To(CounterStateEnum::Finished(Finished {})))
                    } else {
                        None
                    }
                }
                CounterPayload::Decrement(amount) => {
                    self.do_decrement(**amount, data);
                    if data.count <= data.min {
                        Some(Transition::To(CounterStateEnum::Finished(Finished {})))
                    } else {
                        None
                    }
                }
                CounterPayload::CountEvent(event) => match **event {
                    CountEvent::GetCount => {
                        debug!("[Counting] Current count: {}", data.count);
                        //TODO: send count
                        None
                    }
                    CountEvent::Reset => {
                        Some(Transition::To(CounterStateEnum::NotStarted(NotStarted {})))
                    }
                    _ => None,
                },

                _ => None,
            },
            _ => None,
        }
    }

    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Uninit(Uninit {})
    }
}

impl State<CounterComponents> for Finished {
    fn handle_message(
        &self,
        message: &CounterMessageSet,
        data: &mut CounterExtendedState,
    ) -> Option<Transition<CounterStateEnum>> {
        trace!("[Finished] handle_message: {:?}", message);
        match message {
            CounterMessageSet::CounterMessage(msg) => match &msg.payload {
                CounterPayload::CountEvent(event) => match **event {
                    CountEvent::Reset => {
                        data.count = 0;
                        Some(Transition::To(CounterStateEnum::NotStarted(NotStarted {})))
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    fn on_entry(&self, data: &mut CounterExtendedState) {
        info!("Finished!");
        info!("Count is {}", data.count);
    }

    fn on_exit(&self, _data: &mut CounterExtendedState) {
        info!("Finished on_exit");
    }

    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Idle(Idle {})
    }
}

impl State<CounterComponents> for Error {
    fn handle_message(
        &self,
        message: &CounterMessageSet,
        data: &mut CounterExtendedState,
    ) -> Option<Transition<CounterStateEnum>> {
        trace!("[Error] handle_message: {:?}", message);
        match message {
            CounterMessageSet::CounterMessage(msg) => match &msg.payload {
                CounterPayload::CountEvent(event) => match **event {
                    CountEvent::Reset => {
                        data.count = 0;
                        Some(Transition::To(CounterStateEnum::NotStarted(NotStarted {})))
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Idle(Idle {})
    }
}

//TODO: Generate this from a macro

impl State<CounterComponents> for CounterStateEnum {
    fn on_entry(&self, data: &mut CounterExtendedState) {
        match self {
            CounterStateEnum::Uninit(s) => s.on_entry(data),
            CounterStateEnum::NotStarted(s) => s.on_entry(data),
            CounterStateEnum::Idle(s) => s.on_entry(data),
            CounterStateEnum::Counting(s) => s.on_entry(data),
            CounterStateEnum::Finished(s) => s.on_entry(data),
            CounterStateEnum::Error(s) => s.on_entry(data),
        }
    }

    fn on_exit(&self, data: &mut CounterExtendedState) {
        match self {
            CounterStateEnum::Uninit(s) => s.on_exit(data),
            CounterStateEnum::NotStarted(s) => s.on_exit(data),
            CounterStateEnum::Idle(s) => s.on_exit(data),
            CounterStateEnum::Counting(s) => s.on_exit(data),
            CounterStateEnum::Finished(s) => s.on_exit(data),
            CounterStateEnum::Error(s) => s.on_exit(data),
        }
    }

    fn handle_message(
        &self,
        message: &CounterMessageSet,
        data: &mut CounterExtendedState,
    ) -> Option<Transition<CounterStateEnum>> {
        match self {
            CounterStateEnum::Uninit(s) => s.handle_message(message, data),
            CounterStateEnum::NotStarted(s) => s.handle_message(message, data),
            CounterStateEnum::Idle(s) => s.handle_message(message, data),
            CounterStateEnum::Counting(s) => s.handle_message(message, data),
            CounterStateEnum::Finished(s) => s.handle_message(message, data),
            CounterStateEnum::Error(s) => s.handle_message(message, data),
        }
    }

    fn parent(&self) -> CounterStateEnum {
        match self {
            CounterStateEnum::Uninit(s) => s.parent(),
            CounterStateEnum::NotStarted(s) => s.parent(),
            CounterStateEnum::Idle(s) => s.parent(),
            CounterStateEnum::Counting(s) => s.parent(),
            CounterStateEnum::Finished(s) => s.parent(),
            CounterStateEnum::Error(s) => s.parent(),
        }
    }
}
