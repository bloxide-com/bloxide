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
    /// Feature name for paired `#[cfg]` event enum generation.
    /// When set, any mailbox with `feature = "<name>"` is emitted only in
    /// the feature-gated variant.  The non-feature variant uses `generics`;
    /// the feature variant uses `feature_generics`.
    #[serde(default)]
    pub feature: Option<String>,
    /// Generics for the feature-gated variant (e.g. `"<R: BloxRuntime, B: SomeTrait>"`).
    #[serde(default)]
    pub feature_generics: Option<String>,
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
    /// Feature gate for this mailbox. When set, the mailbox variant (and its
    /// associated From/EventTag/accessor impls) is emitted only under
    /// `#[cfg(feature = "...")]`. When `None`, the mailbox is always emitted.
    #[serde(default)]
    pub feature: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TopologyConfig {
    pub states: Vec<StateConfig>,
    /// Declarative transitions grouped by state. When present, the codegen
    /// emits complete `StateFns` constants with raw `StateRule` struct literals
    /// directly from TOML — no hand-written actions needed.
    #[serde(default)]
    pub transitions: Vec<TransitionConfig>,
    /// Entry actions per state.
    #[serde(default)]
    pub entry: Vec<EntryExitConfig>,
    /// Exit actions per state.
    #[serde(default)]
    pub exit: Vec<EntryExitConfig>,
    /// Raw `use` statements for the spec_skeleton module. These import the
    /// action functions referenced in transitions/entry/exit.
    /// e.g. `["bloxide_child_management::{start_children, stop_all_children, handle_done_or_failed, ...}"]`
    #[serde(default)]
    pub spec_imports: Vec<String>,
    /// Feature-gated raw `use` statements for the spec_skeleton module.
    /// These imports appear only in the feature variant.
    #[serde(default)]
    pub feature_spec_imports: Vec<String>,
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
    /// Feature gate for this transition. When set, the transition is emitted
    /// only under `#[cfg(feature = "...")]`.
    #[serde(default)]
    pub feature: Option<String>,
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
    /// Feature gate for this entry/exit. When set, the entry/exit is emitted
    /// only under `#[cfg(feature = "...")]`.
    #[serde(default)]
    pub feature: Option<String>,
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
    /// Extra impl blocks emitted after the context struct, wrapped with the
    /// appropriate generics for each variant. Each entry is a raw impl body
    /// WITHOUT the `impl<...>` header — the codegen wraps it as:
    /// `impl<VARIANT_GENERICS> <body>`
    /// where `<body>` is the entry content (e.g. `HasPending for Ctx<R> { ... }`).
    #[serde(default)]
    pub extra_impls: Vec<String>,
    #[serde(default)]
    pub imports: Vec<String>,
    /// Composable context crate declarations.
    ///
    /// Each entry pulls in one or more traits from an external crate and
    /// optionally contributes fields to the generated context struct.
    /// See `spec/architecture/18-composable-context-crates.md`.
    #[serde(default)]
    pub uses: Vec<ContextUse>,

    // ── Feature-gating (paired `#[cfg]` generation) ───────────────────────
    //
    // When `feature` is set, the codegen emits TWO variants of the context
    // struct (and the spec_skeleton): one under `#[cfg(not(feature = "..."))]`
    // and one under `#[cfg(feature = "...")]`.
    //
    // Fields/uses with `feature = "<name>"` appear only in the feature variant.
    // The non-feature variant uses `generics`; the feature variant uses
    // `feature_generics`.
    /// Feature name for paired generation (e.g. `"dynamic"`).
    #[serde(default)]
    pub feature: Option<String>,
    /// Generics for the feature-gated variant (e.g. `"<R: BloxRuntime, B: SomeTrait>"`).
    #[serde(default)]
    pub feature_generics: Option<String>,
    /// Extra `where`-clause predicates for the feature-gated variant.
    #[serde(default)]
    pub feature_where: Vec<String>,
    /// Extra imports for the feature-gated variant (raw `use` statements).
    #[serde(default)]
    pub feature_imports: Vec<String>,

    // ── Event type info (when no `[event]` section is present) ─────────────
    //
    // When the event enum is hand-written (not generated), the context config
    // provides the event type name and generics so the spec_skeleton can
    // reference them.
    /// Event type name (e.g. `"SupervisorEvent"`).  Required when there is
    /// no `[event]` section but a spec_skeleton is needed.
    #[serde(default)]
    pub event_name: Option<String>,
    /// Event generics for the non-feature variant (e.g. `"<R>"`).
    #[serde(default)]
    pub event_generics: Option<String>,
    /// Event generics for the feature-gated variant (e.g. `"<R, F>"`).
    #[serde(default)]
    pub feature_event_generics: Option<String>,

    // ── Mailboxes type (when no `[event]` section is present) ─────────────
    //
    // When the event enum is hand-written, the Mailboxes associated type
    // must be specified as a raw type expression.
    /// Mailboxes type for the non-feature variant, as a raw string.
    /// e.g. `"(Rt::Stream<ChildLifecycleEvent>, Rt::Stream<SupervisorControl<R>>)"`.
    #[serde(default)]
    pub mailboxes_type: Option<String>,
    /// Mailboxes type for the feature-gated variant, as a raw string.
    /// e.g. `"crate::dynamic_mailboxes::SupervisorMailboxes<R, Rt, F>"`.
    #[serde(default)]
    pub feature_mailboxes_type: Option<String>,
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

    /// `#[provides(TraitPath)]` annotation for the BloxCtx derive macro.
    /// Generates `impl TraitPath for Struct` that returns `&self.field`.
    /// May include associated type bindings.
    /// Used when the trait name doesn't follow the `Has{FieldName}` convention.
    #[serde(default)]
    pub provides: Option<String>,

    /// `#[provides_mut(TraitPath, method_name)]` annotation for BloxCtx.
    /// Generates `impl TraitPath for Struct` with a mutable accessor.
    #[serde(default)]
    pub provides_mut: Option<String>,

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

    /// Feature gate for this entire `uses` entry. When set, the entry's
    /// trait imports, fields, and impl_macro call are emitted only under
    /// `#[cfg(feature = "...")]`.
    #[serde(default)]
    pub feature: Option<String>,
}

/// A `[[context.uses.fields]]` entry — a field contributed by a multi-field
/// context crate.
#[derive(Debug, Deserialize, Clone)]
pub struct ContextUseField {
    pub name: String,
    pub ty: String,
    /// Field role: `"state"` (zero-initialized) or `"ctor"` (constructor param).
    pub role: Option<String>,
    /// `#[provides(TraitPath)]` annotation for the BloxCtx derive macro.
    /// Generates `impl TraitPath for Struct` that returns `&self.field`.
    #[serde(default)]
    pub provides: Option<String>,
    /// `#[provides_mut(TraitPath, method_name)]` annotation for BloxCtx.
    /// Generates `impl TraitPath for Struct` with a mutable accessor.
    #[serde(default)]
    pub provides_mut: Option<String>,
    /// Feature gate for this sub-field. When set, the field is emitted only
    /// under `#[cfg(feature = "...")]`.
    #[serde(default)]
    pub feature: Option<String>,
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
    /// Format: `<MsgType>::<VariantName>`
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
    /// Named ref selector when `source = "actor"`. Defaults to `"primary"`.
    /// The codegen maintains a symbol table mapping `(actor, field)` to
    /// variable idents. Common values: `"primary"` (default channel ref),
    /// `"control"` (supervisor's control_ref), `"notify"` (supervisor's
    /// notify_ref).
    #[serde(default)]
    pub field: Option<String>,
    /// Mailbox index for multi-mailbox actors (0-based).
    /// When absent, defaults to 0 (the primary mailbox).
    pub mailbox: Option<usize>,
    /// Index for `source = "self_secondary"` — which additional
    /// channel ref to use from a multi-mailbox actor's `channels!` call.
    /// Defaults to 1 (the second channel).
    #[serde(default)]
    pub index: Option<usize>,
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
