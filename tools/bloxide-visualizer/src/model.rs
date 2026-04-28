// Copyright 2025 Bloxide, all rights reserved
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BloxSpec {
    pub name: String,
    pub states: Vec<State>,
    pub events: Vec<Event>,
    pub handlers: Vec<Handler>,
    pub entry_exit: HashMap<String, EntryExit>,
    pub message_sets: Vec<MessageSet>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub name: String,
    pub kind: StateKind,
    pub parent: Option<String>,
    pub description: String,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StateKind {
    Leaf,
    Composite,
    Terminal,
    Error,
}

impl StateKind {
    pub fn is_leaf(&self) -> bool {
        matches!(
            self,
            StateKind::Leaf | StateKind::Terminal | StateKind::Error
        )
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            StateKind::Leaf => "",
            StateKind::Composite => "◇",
            StateKind::Terminal => "◆",
            StateKind::Error => "◈",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub message_set: String,
    pub variant: String,
    pub full_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageSet {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Handler {
    pub state: String,
    pub event: String,
    pub label: String,
    pub actions: Vec<String>,
    pub guard: Guard,
    pub target: Target,
    pub source: HandlerSource,
    pub on_entry: Vec<String>,
    pub on_exit: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HandlerSource {
    Explicit,
    Inherited(String), // from parent state name
    Dropped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Target {
    Stay,
    Transition(String),
    Reset,
}

impl Target {
    pub fn display(&self) -> String {
        match self {
            Target::Stay => "stay".to_string(),
            Target::Transition(s) => s.clone(),
            Target::Reset => "reset".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Guard {
    pub description: String,
    pub raw: String,
    pub branches: Vec<GuardBranch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardBranch {
    pub condition: String,
    pub target: Target,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryExit {
    pub on_entry: Vec<String>,
    pub on_exit: Vec<String>,
}

impl BloxSpec {
    pub fn leaf_states(&self) -> Vec<&State> {
        self.states.iter().filter(|s| s.kind.is_leaf()).collect()
    }

    pub fn composite_states(&self) -> Vec<&State> {
        self.states
            .iter()
            .filter(|s| matches!(s.kind, StateKind::Composite))
            .collect()
    }

    pub fn state_by_name(&self, name: &str) -> Option<&State> {
        self.states.iter().find(|s| s.name == name)
    }

    pub fn handler_for(&self, state: &str, event: &str) -> Option<&Handler> {
        self.handlers
            .iter()
            .find(|h| h.state == state && h.event == event)
    }

    pub fn events_for_set(&self, set_name: &str) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|e| e.message_set == set_name)
            .collect()
    }

    pub fn message_sets_for_events(&self) -> Vec<MessageSet> {
        let mut sets: HashMap<String, Vec<String>> = HashMap::new();
        for event in &self.events {
            sets.entry(event.message_set.clone())
                .or_default()
                .push(event.variant.clone());
        }
        sets.into_iter()
            .map(|(name, variants)| MessageSet { name, variants })
            .collect()
    }
}
