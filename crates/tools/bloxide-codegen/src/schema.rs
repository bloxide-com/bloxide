// Copyright 2025 Bloxide, all rights reserved
use serde::Deserialize;
use std::collections::BTreeMap;

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
    /// When true, generated structs/enum also derive `Copy`.
    /// Defaults to `false` — only `Debug` and `Clone` are derived.
    #[serde(default)]
    pub copy: bool,
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
    pub debug: Option<bool>, // default true; deprecated — use `derives` instead
    /// Custom derive trait paths to apply to the generated event enum.
    /// When set, overrides `debug`. An empty list means no derives at all.
    /// When unset (None), falls back to `debug` for backward compatibility.
    #[serde(default)]
    pub derives: Option<Vec<String>>,
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
    /// Legacy: list of handler function names (one per state), used when
    /// transitions are not declared in TOML. The codegen emits a
    /// `handler_table` macro that references hand-written `StateFns` constants.
    pub handler_fns: Option<Vec<String>>,
    /// Declarative transitions grouped by state. When present, the codegen
    /// emits complete `StateFns` constants with `transitions!` macro
    /// invocations from TOML — no hand-written actions needed.
    #[serde(default)]
    pub transitions: Vec<TransitionConfig>,
    /// Entry actions per state.
    #[serde(default)]
    pub entry: Vec<EntryExitConfig>,
    /// Exit actions per state.
    #[serde(default)]
    pub exit: Vec<EntryExitConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TransitionConfig {
    /// Which state handles this transition.
    pub state: String,
    /// Event pattern, e.g. "PingPongMsg::Ping(_)" or "PingPongMsg::A(_) | PingPongMsg::B(_)".
    pub event: String,
    /// Target: a state name, or "stay", "reset", "fail".
    pub target: String,
    /// Action functions to call (function paths, e.g. "Self::forward_ping" or "send_pong").
    #[serde(default)]
    pub actions: Vec<String>,
    /// Guard conditions: each entry is a "condition => target" pair.
    /// When present, `target` is the fallback (the `_` arm).
    #[serde(default)]
    pub guards: Vec<GuardConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GuardConfig {
    /// Guard condition expression, e.g. "ctx.round() >= MAX_ROUNDS".
    pub condition: String,
    /// Target when guard passes: a state name, or "stay", "reset", "fail".
    pub target: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntryExitConfig {
    /// Which state this entry/exit applies to.
    pub state: String,
    /// Action functions to call.
    pub actions: Vec<String>,
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
    /// Extra `where`-clause predicates appended to the `MachineSpec` impl.
    /// e.g. `["B: Default", "B::Round: Into<u32>"]`
    #[serde(default)]
    pub extra_where: Vec<String>,
    /// Body of `on_init_entry` as a raw string (inserted verbatim).
    #[serde(default)]
    pub on_init: Option<String>,
    #[serde(default)]
    pub fields: Vec<ContextFieldConfig>,
    #[serde(default)]
    pub imports: Vec<String>,
    /// Composable context crate declarations.
    ///
    /// Each entry pulls in one or more traits from an external crate and
    /// optionally contributes fields to the generated context struct.
    /// See `spec/architecture/18-composable-context-crates.md`.
    #[serde(default)]
    pub uses: Vec<ContextUse>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ContextFieldConfig {
    pub name: String,
    pub ty: String,
    pub delegates: Option<Vec<String>>,
    /// Explicit field role for codegen: `"self_id"`, `"accessor"`, `"ctor"`,
    /// `"state"`, or `"delegate"`. When absent, the codegen infers the role
    /// from naming conventions (the legacy behaviour).
    pub role: Option<String>,
}

/// A `[[context.uses]]` entry — pulls traits and fields from a composable
/// context crate into the generated context struct.
///
/// # Variants
///
/// **Single-field accessor** (from a service crate like `bloxide-messaging`):
/// ```toml
/// [[context.uses]]
/// crate = "bloxide_messaging"
/// trait = "HasPeerRef<R, PingPongMsg>"
/// field = "peer_ref"
/// field_type = "ActorRef<PingPongMsg, R>"
/// role = "ctor"
/// ```
///
/// **Delegatable behavior trait** (used via `#[delegates(...)]`):
/// ```toml
/// [[context.uses]]
/// crate = "blox_ctx_rounds"
/// trait = "CountsRounds"
/// delegatable = true
/// ```
///
/// **Multi-field trait** (domain context crate with impl macro):
/// ```toml
/// [[context.uses]]
/// crate = "blox_ctx_workers"
/// traits = ["HasWorkers<R>", "HasWorkerFactory<R>"]
/// impl_macro = "impl_has_workers"
///
///   [[context.uses.fields]]
///   name = "worker_refs"
///   ty = "Vec<ActorRef<WorkerMsg, R>>"
///   role = "state"
/// ```
#[derive(Debug, Deserialize, Clone)]
pub struct ContextUse {
    /// Crate name (underscores, e.g. `bloxide_messaging`).
    /// Renamed from `crate_name` because `crate` is a Rust keyword.
    #[serde(rename = "crate")]
    pub crate_name: String,

    /// Single trait provided by this crate (e.g. `"HasPeerRef<R, PingPongMsg>"`).
    /// Mutually exclusive with `traits`; use whichever fits the entry.
    #[serde(rename = "trait")]
    pub trait_: Option<String>,

    /// Multiple traits provided by this crate (for multi-field context crates).
    /// Mutually exclusive with `trait`.
    #[serde(default)]
    pub traits: Vec<String>,

    /// Field name for single-field accessor traits (e.g. `"peer_ref"`).
    pub field: Option<String>,

    /// Field type for single-field accessor traits (e.g. `"ActorRef<PingPongMsg, R>"`).
    pub field_type: Option<String>,

    /// Field role: `"accessor"`, `"ctor"`, or `"state"`.
    /// Controls how the codegen emits the field and attributes.
    pub role: Option<String>,

    /// When `true`, the trait is `#[delegatable]` and the codegen should emit
    /// `__delegate_{Trait}` imports alongside the trait import.
    #[serde(default)]
    pub delegatable: bool,

    /// Impl macro name for multi-field traits (e.g. `"impl_has_workers"`).
    /// The codegen emits `{impl_macro}!({CtxName}<R>);` after the struct.
    pub impl_macro: Option<String>,

    /// Sub-fields for multi-field traits.
    #[serde(default)]
    pub fields: Vec<ContextUseField>,
}

/// A `[[context.uses.fields]]` entry — a field contributed by a multi-field
/// context crate.
#[derive(Debug, Deserialize, Clone)]
pub struct ContextUseField {
    pub name: String,
    pub ty: String,
    /// Field role: `"state"` (zero-initialized) or `"ctor"` (constructor param).
    pub role: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WiringConfig {
    pub runtime: String,
    #[serde(default)]
    pub channels: Vec<ChannelConfig>,
    #[serde(default)]
    pub actors: Vec<WiringActorConfig>,
    #[serde(default)]
    pub connections: Vec<WiringConnectionConfig>,
    #[serde(default)]
    pub supervisors: Vec<WiringSupervisorConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WiringActorConfig {
    pub blox: String,
    pub name: String,
    pub behavior: Option<String>,
    #[serde(default)]
    pub behavior_traits: Vec<String>,
    #[serde(default)]
    pub context_fields: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WiringConnectionConfig {
    pub from: String,
    pub to: String,
    pub message: String,
    pub channel_capacity: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WiringSupervisorConfig {
    pub name: String,
    pub strategy: String,
    #[serde(default)]
    pub children: Vec<WiringSupervisorChildConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WiringSupervisorChildConfig {
    pub actor: String,
    pub restart_max: Option<u32>,
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

// ---------------------------------------------------------------------------
// system.toml — declarative wiring manifest
//
// A separate TOML file that describes the actor system topology: which actor
// instances to create, how to inject constructor params, how to wire the
// supervisor tree, and which runtime to target. The codegen turns this into a
// complete `main.rs` binary.
//
// See `spec/architecture/19-declarative-wiring.md`.
// ---------------------------------------------------------------------------

/// Top-level wiring manifest (`system.toml`).
///
/// ```toml
/// [system]
/// runtime = "tokio"
///
/// [[actors]]
/// name = "ping"
/// blox = "ping-blox"
/// # ...
///
/// [[supervision]]
/// supervisor = "bloxide-supervisor"
/// strategy = "one_for_one"
/// children = ["ping", "pong"]
/// ```
#[derive(Debug, Deserialize, Clone)]
pub struct SystemConfig {
    /// Runtime selection and global system settings.
    pub system: SystemMeta,
    /// Actor instances to create.
    #[serde(default)]
    pub actors: Vec<ActorInstance>,
    /// Supervisor tree definitions.
    #[serde(default)]
    pub supervision: Vec<SupervisionConfig>,
}

/// `[system]` table — runtime selection.
#[derive(Debug, Deserialize, Clone)]
pub struct SystemMeta {
    /// Target runtime: `"tokio"`, `"embassy"`, or `"test"`.
    pub runtime: String,
    /// Optional system name (used as the binary name in generated output).
    pub name: Option<String>,
}

/// A bootstrap message to send to an actor after the supervisor starts.
#[derive(Debug, Deserialize, Clone)]
pub struct BootstrapMessage {
    /// Full message variant path, e.g. "CounterMsg::Tick" or "PoolMsg::SpawnWorker".
    /// Format: "<MsgType>::<VariantName>"
    pub message: String,
    /// Optional payload fields as key-value pairs.
    /// For unit-like variants (e.g. Tick), omit this.
    /// For struct variants with fields, provide field values:
    ///   payload = { task_id = 0 }
    #[serde(default)]
    pub payload: Option<toml::Table>,
}

/// An `[[actors]]` entry — one actor instance in the system.
#[derive(Debug, Deserialize, Clone)]
pub struct ActorInstance {
    /// Instance name (unique within the system).
    pub name: String,
    /// Blox crate name (e.g. `"ping-blox"`).
    pub blox: String,
    /// Behavior type name (e.g. `"DemoBehavior"`).
    /// Required when the blox context has a generic behavior parameter `B`.
    pub behavior: Option<String>,
    /// Actor kind: "timer" for timer service actors, None for normal blox actors.
    pub kind: Option<String>,
    /// Crate that provides the behavior type (e.g. "embassy_demo_impl").
    pub behavior_impl: Option<String>,
    /// Channel capacity for this actor's primary mailbox (default 16).
    pub channel_capacity: Option<usize>,
    /// Traits the behavior type implements (e.g. `["CountsRounds", "HasCurrentTimer"]`).
    #[serde(default)]
    pub behavior_traits: Vec<String>,
    /// Bootstrap messages to send after supervisor starts.
    #[serde(default)]
    pub bootstrap: Vec<BootstrapMessage>,

    /// Constructor param injections: field name → source mapping.
    ///
    /// ```toml
    /// [actors.inject]
    /// self_ref = { source = "self" }
    /// peer_ref = { source = "actor", actor = "pong" }
    /// ```
    #[serde(default)]
    pub inject: BTreeMap<String, InjectSource>,
    /// Spawn factory for dynamic actors.
    ///
    /// ```toml
    /// [actors.spawn_factory]
    /// crate = "tokio_pool_demo_impl"
    /// function = "spawn_worker_tokio"
    /// ```
    pub spawn_factory: Option<SpawnFactoryConfig>,
}

/// A value in `[actors.inject]` — where a constructor param's handle comes from.
///
/// When `source = "factory"`, `crate` and `function` identify the factory
/// function to inject as a function pointer.
#[derive(Debug, Deserialize, Clone)]
pub struct InjectSource {
    /// `"self"` — create a channel for this actor and inject its ref.
    /// `"actor"` — use another actor's channel ref (requires `actor` field).
    pub source: String,
    /// Crate name for factory injection (when `source = "factory"`).
    #[serde(rename = "crate")]
    pub crate_name: Option<String>,
    /// Function name for factory injection (when `source = "factory"`).
    pub function: Option<String>,
    /// Name of the source actor when `source = "actor"`.
    pub actor: Option<String>,
    /// Mailbox index for multi-mailbox actors (0-based).
    /// When absent, defaults to 0 (the primary mailbox).
    pub mailbox: Option<usize>,
}

/// `[actors.spawn_factory]` — dynamic spawn factory reference.
#[derive(Debug, Deserialize, Clone)]
pub struct SpawnFactoryConfig {
    /// Crate that provides the factory function (e.g. `"tokio_pool_demo_impl"`).
    #[serde(rename = "crate")]
    pub crate_name: String,
    /// Factory function path (e.g. `"spawn_worker_tokio"`).
    pub function: String,
}

/// A `[[supervision]]` entry — one supervisor group.
#[derive(Debug, Deserialize, Clone)]
pub struct SupervisionConfig {
    /// Supervisor crate/spec name (e.g. `"bloxide-supervisor"`).
    pub supervisor: String,
    /// Restart strategy: `"one_for_one"`, `"one_for_all"`, or `"rest_for_one"`.
    pub strategy: String,
    /// Child actor names managed by this supervisor.
    pub children: Vec<String>,
    /// Per-child policies.
    ///
    /// ```toml
    /// [supervision.policies]
    /// ping = { restart = { max = 1 } }
    /// pong = { stop = true }
    /// ```
    #[serde(default)]
    pub policies: BTreeMap<String, ChildPolicyConfig>,
    /// Optional health-check interval in milliseconds.
    pub health_check_interval_ms: Option<u64>,
}

/// A value in `[supervision.policies]` — restart or stop policy for a child.
#[derive(Debug, Deserialize, Clone)]
pub struct ChildPolicyConfig {
    /// Restart policy with max restart count.
    pub restart: Option<RestartPolicy>,
    /// When `true`, the supervisor stops the entire group when this child terminates.
    pub stop: Option<bool>,
}

/// Restart policy parameters.
#[derive(Debug, Deserialize, Clone)]
pub struct RestartPolicy {
    /// Maximum restart attempts before escalation.
    pub max: u32,
}
