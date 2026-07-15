// Copyright 2025 Bloxide, all rights reserved
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct BloxConfig {
    pub actor: Option<ActorConfig>,
    pub messages: Option<Vec<MessageEnumConfig>>,
    pub event: Option<EventConfig>,
    pub topology: Option<TopologyConfig>,
    pub context: Option<ContextConfig>,
    pub wiring: Option<WiringConfig>,
    pub mailboxes: Option<MailboxesConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ActorConfig {
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessageEnumConfig {
    pub name: String,
    pub visibility: Option<String>,
    pub variants: Vec<MessageVariantConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessageVariantConfig {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<MessageFieldConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessageFieldConfig {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EventConfig {
    pub name: String,
    pub generics: Option<String>, // e.g. "<R: BloxRuntime>"
    #[serde(default)]
    pub debug: Option<bool>, // default true
    pub mailboxes: Vec<MailboxConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MailboxConfig {
    pub variant: String,
    pub message: String,
    pub message_path: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TopologyConfig {
    pub states: Vec<StateConfig>,
    pub handler_fns: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StateConfig {
    pub name: String,
    pub composite: Option<bool>,
    pub parent: Option<String>,
    pub initial: Option<bool>,
    pub terminal: Option<bool>,
    pub error: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ContextConfig {
    pub name: String,
    pub generics: Option<String>,
    pub actions_crate: Option<String>,
    #[serde(default)]
    pub fields: Vec<ContextFieldConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ContextFieldConfig {
    pub name: String,
    pub ty: String,
    pub delegates: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WiringConfig {
    pub runtime: String,
    pub channels: Vec<ChannelConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChannelConfig {
    pub message: String,
    pub capacity: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MailboxesConfig {
    pub max_arity: usize,
}
