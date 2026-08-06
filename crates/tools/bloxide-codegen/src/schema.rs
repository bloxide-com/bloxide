// Copyright 2025 Bloxide, all rights reserved
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct BloxConfig {
    pub actor: Option<ActorConfig>,
    pub messages: Option<Vec<MessageEnumConfig>>,
    pub event: Option<EventConfig>,
    pub topology: Option<TopologyConfig>,
    pub context: Option<ContextConfig>,
    pub mailboxes: Option<MailboxesConfig>,
    /// Crate packaging for pure-TOML bloxes (blox.toml-only sources under
    /// `bloxes/`): features, extra dependencies, and dev-dependencies for the
    /// crate materialized into `target/bloxide-generated/crates/<name>`.
    /// Ignored for in-crate blox.toml files (stdlib crates with their own
    /// hand-written Cargo.toml).
    #[serde(default)]
    pub package: Option<PackageConfig>,
    /// Crate-root constants emitted into the generated lib.rs (referenced by
    /// guards via `spec_imports` entries like `crate::MAX_ROUNDS`).
    #[serde(default)]
    pub consts: Vec<ConstConfig>,
}

/// A `[package]` table — packaging metadata for a materialized blox crate.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct PackageConfig {
    /// Crate description: emitted as the Cargo.toml `description` and the
    /// lib.rs `//!` doc line.
    #[serde(default)]
    pub description: Option<String>,
    /// Cargo features (`[features]` table): feature name → list of feature
    /// strings (e.g. `std = ["bloxide-core/std", "bloxide-timer/std"]`).
    #[serde(default)]
    pub features: BTreeMap<String, Vec<String>>,
    /// Extra dependencies not derivable from the blox.toml sections
    /// (message paths, context imports, spec imports, action crates).
    /// Resolved as path dependencies via the workspace root's
    /// `[workspace.dependencies]` table.
    #[serde(default)]
    pub dependencies: BTreeMap<String, DepSpec>,
    /// Dev-dependencies for the crate's integration tests (the blox's
    /// `tests/` directory). Resolved as path dependencies the same way.
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: BTreeMap<String, DepSpec>,
}

/// A dependency spec in `[package.dependencies]` / `[package.dev-dependencies]`.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct DepSpec {
    /// Features to enable on the dependency.
    #[serde(default)]
    pub features: Vec<String>,
}

/// A `[[consts]]` entry — a crate-root constant in the generated lib.rs.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ConstConfig {
    pub name: String,
    pub ty: String,
    /// Literal value, emitted verbatim (e.g. `"5"`, `"2"`).
    pub value: String,
    /// Optional doc comment emitted above the const (one line; emitted as
    /// `/// ...`).
    #[serde(default)]
    pub doc: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ActorConfig {
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct MessageVariantConfig {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<MessageFieldConfig>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct MessageFieldConfig {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
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
    /// Custom derive trait paths to apply to the generated event enum.
    /// When `None`, defaults to `["Debug"]`. An empty list means no derives
    /// at all.
    #[serde(default)]
    pub derives: Option<Vec<String>>,
    pub mailboxes: Vec<MailboxConfig>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct MailboxConfig {
    pub variant: String,
    pub message: String,
    pub message_path: Option<String>,
    /// Feature gate for this mailbox. When set, the mailbox variant (and its
    /// associated From/EventTag/accessor impls) is emitted only under
    /// `#[cfg(feature = "...")]`. When `None`, the mailbox is always emitted.
    #[serde(default)]
    pub feature: Option<String>,
    /// Full variant set of the message enum carried by this mailbox
    /// (e.g. `["Started", "Stopped", ...]`). Used for exhaustiveness
    /// analysis only: when a state's earlier rules already cover every
    /// declared variant, the codegen omits that state's catch-all rule for
    /// this event variant (e.g. `SupervisorEvent::Child(_)`) as unreachable
    /// dead code. When empty or absent, catch-alls are always kept.
    #[serde(default)]
    pub variants: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct TopologyConfig {
    pub states: Vec<StateConfig>,
    /// Declarative transitions grouped by state. When present, the codegen
    /// emits complete `StateFns` constants with raw `StateRule` struct literals
    /// directly from TOML — no hand-written actions needed.
    ///
    /// `state = "root"` marks a root-level rule: evaluated when a domain event
    /// bubbles past all user-declared states to the engine-implicit VirtualRoot.
    /// Root rules are emitted as the `ROOT_RULES` constant plus a
    /// `root_transitions()` override in the generated `MachineSpec` impl.
    /// `"root"` is a reserved keyword and cannot name a user state.
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
#[serde(deny_unknown_fields)]
pub struct TransitionConfig {
    /// Which state handles this transition. The reserved keyword `"root"`
    /// marks a root-level rule (VirtualRoot fallback for domain events that
    /// bubble past all user states); it cannot name a user state.
    pub state: String,
    /// Event pattern, e.g. "PingPongMsg::Ping(_)" or "PingPongMsg::A(_) | PingPongMsg::B(_)".
    pub event: String,
    /// Target: a state name, or "stay", "reset", "stop", "done", "fail".
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

/// Reserved `state` value marking a root-level transition rule. Root rules
/// have no owning user state — they live at the engine-implicit VirtualRoot
/// and are evaluated when a domain event bubbles past every user-declared
/// state. Lifecycle commands are intercepted by the engine before user
/// states, so root rules are for domain events only. `"root"` cannot name a
/// user state.
pub const ROOT_STATE_KEYWORD: &str = "root";

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct GuardConfig {
    /// Guard condition expression, e.g. "ctx.round >= MAX_ROUNDS".
    /// `ctx` is `&Ctx` — direct field access, no accessor methods.
    /// Use "_" for an explicit wildcard fallback arm.
    pub condition: String,
    /// Target when guard passes: a state name, or "stay", "reset", "stop", "done", "fail".
    pub target: String,
}
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct StateConfig {
    pub name: String,
    pub composite: Option<bool>,
    pub parent: Option<String>,
    pub initial: Option<bool>,
    pub error: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ContextConfig {
    pub name: String,
    pub generics: Option<String>,
    /// Extra `where`-clause predicates appended to the `MachineSpec` impl.
    /// e.g. `["R: SomeExtraBound"]`
    #[serde(default)]
    pub extra_where: Vec<String>,
    /// Body of `on_init_entry` as a raw string (inserted verbatim).
    #[serde(default)]
    pub on_init: Option<String>,
    /// Body of `on_init_entry` for the feature-gated variant (paired `#[cfg]`
    /// generation). Use this to reset feature-gated state fields, which do not
    /// exist in the non-feature variant. Falls back to `on_init` when unset.
    #[serde(default)]
    pub feature_on_init: Option<String>,
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
    /// Each entry pulls in a field (or several) from an external crate into
    /// the generated context struct.
    /// See `spec/architecture/13-composable-context-crates.md`.
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
    /// e.g. `"(Rt::Stream<ChildLifecycleEvent>, Rt::Stream<ChildCtrl<R>>)"`.
    #[serde(default)]
    pub mailboxes_type: Option<String>,
    /// Mailboxes type for the feature-gated variant, as a raw string.
    /// e.g. `"crate::dynamic_mailboxes::SupervisorMailboxes<R, Rt, F>"`.
    #[serde(default)]
    pub feature_mailboxes_type: Option<String>,

    /// State fields declared directly in `[[context.fields]]`.
    /// These are fields that were previously in `B` (the behavior trait)
    /// and are now inlined into the context struct.
    #[serde(default)]
    pub fields: Vec<ContextFieldConfig>,

    /// Action declarations for system codegen.
    /// Each entry describes how to generate a concrete action closure.
    #[serde(default)]
    pub actions: Vec<ContextActionConfig>,
}

/// A `[[context.fields]]` entry — a state field declared directly in the
/// context struct (not via a composable crate trait).
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ContextFieldConfig {
    pub name: String,
    pub r#type: String,
    #[serde(default)]
    pub default: Option<String>,
}

/// A `[[context.actions]]` entry — declares an action's signature for
/// the system codegen to generate concrete action closures.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextActionConfig {
    /// Action name (without `Self::` prefix), e.g. `"process_work"`.
    pub name: String,
    /// Crate where the action function lives (for non-impl actions).
    /// e.g. `"blox_ctx_rounds"`. Omitted when `impl_required = true`.
    #[serde(default, rename = "crate")]
    pub crate_name: Option<String>,
    /// Context fields the action needs, with access mode suffix:
    /// `"field:mut"` (mutable ref), `"field:ref"` (shared ref),
    /// `"field"` (copy/owned).
    #[serde(default)]
    pub fields: Vec<String>,
    /// Optional event payload variable name to extract from the event.
    /// e.g. `"do_work"` means the action receives the `DoWork` payload.
    #[serde(default)]
    pub event_payload: Option<String>,
    /// When true, the transition action closure passes the full event
    /// reference (`_ev`) as the last argument to the action function.
    /// Use this when the action function takes `ev: &Event` directly
    /// (rather than a destructured payload via `event_payload`).
    /// Entry/exit actions ignore this field (they never receive an event).
    #[serde(default)]
    pub event_arg: bool,
    /// Whether the action function lives in an impl crate (true) or
    /// a context/peer crate (false).
    #[serde(default)]
    pub impl_required: bool,
    /// Feature gate for this action (e.g. `"dynamic"`).
    #[serde(default)]
    pub feature: Option<String>,
    /// Override the function name to call. If not set, the `name` field is
    /// used as the function name. This allows the topology to use a logical
    /// action name (e.g. `Self::forward_ping`) while calling a differently-
    /// named function in the crate (e.g. `send_ping`).
    #[serde(default)]
    pub fn_name: Option<String>,
    /// Optional module path segment between the crate path and the function
    /// name. When set, the generated call is `<crate>::<module>::<fn_name>`
    /// instead of `<crate>::<fn_name>`. e.g. `module = "actions"` produces
    /// `crate::actions::start_children(...)`.
    #[serde(default)]
    pub module: Option<String>,
    /// Declared return type of the action function. The only recognized
    /// value is `"ActionResult"` — the transition wrapper then emits the
    /// call bare, skipping the `ActionResult::from(...)` normalization
    /// (which would be a same-type conversion flagged by clippy). When
    /// omitted, the wrapper is emitted (contract: the function returns
    /// `ActionResult`, `Result<(), E>`, or `()`).
    #[serde(default)]
    pub returns: Option<String>,
}

/// A `[[context.uses]]` entry — pulls traits and fields from a composable
/// context crate into the generated context struct.
///
/// # Variants
///
/// **Single-field** (from a service crate like `bloxide-messaging`):
/// ```toml
/// [[context.uses]]
/// field = "peer_ref"
/// field_type = "ActorRef<PingPongMsg, R>"
/// role = "ctor"
/// ```
///
/// **Multi-field** (domain context crate):
/// ```toml
/// [[context.uses]]
///
///   [[context.uses.fields]]
///   name = "worker_refs"
///   ty = "Vec<ActorRef<WorkerMsg, R>>"
///   role = "state"
/// ```
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ContextUse {
    /// Field name for single-field context crates (e.g. `"peer_ref"`).
    pub field: Option<String>,

    /// Field type for single-field context crates (e.g. `"ActorRef<PingPongMsg, R>"`).
    pub field_type: Option<String>,

    /// Field role: `"ctor"` (constructor parameter) or `"state"`
    /// (zero-initialized state field). These are the only values.
    pub role: Option<String>,

    /// Sub-fields for multi-field context crates.
    #[serde(default)]
    pub fields: Vec<ContextUseField>,

    /// Feature gate for this entire `uses` entry. When set, the entry's
    /// imports and fields are emitted only under
    /// `#[cfg(feature = "...")]`.
    #[serde(default)]
    pub feature: Option<String>,
}

/// A `[[context.uses.fields]]` entry — a field contributed by a multi-field
/// context crate.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ContextUseField {
    pub name: String,
    pub ty: String,
    /// Field role: `"state"` (zero-initialized) or `"ctor"` (constructor param).
    pub role: Option<String>,
    /// Feature gate for this sub-field. When set, the field is emitted only
    /// under `#[cfg(feature = "...")]`.
    #[serde(default)]
    pub feature: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
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
// See `spec/architecture/14-declarative-wiring.md`.
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
/// strategy = "when_any_done"
/// children = ["ping", "pong"]
/// ```
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct SystemMeta {
    /// Target runtime: `"tokio"`, `"embassy"`, or `"test"`.
    pub runtime: String,
    /// Optional system name (used as the binary name in generated output).
    pub name: Option<String>,
}

/// A bootstrap message to send to an actor after the supervisor starts.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct ActorInstance {
    /// Instance name (unique within the system).
    pub name: String,
    /// Blox crate name (e.g. `"ping-blox"`).
    pub blox: String,
    /// Actor kind: "timer" for timer service actors, "dynamic" for dynamically
    /// spawned actors (concrete spec generated, but no channels/task/bootstrap
    /// in main.rs — the impl crate's spawn function handles construction).
    /// None (or absent) for normal static actors.
    pub kind: Option<String>,
    /// Impl crate for concrete action closures (Phase 3 system codegen).
    /// When set, actions with `impl_required = true` resolve functions from this crate.
    pub impl_crate: Option<String>,
    /// Channel capacity for this actor's primary mailbox (default 16).
    pub channel_capacity: Option<usize>,
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

    /// Features enabled for this actor's blox crate.
    ///
    /// The codegen uses this to filter feature-gated mailboxes: only
    /// mailboxes whose `feature` field is `None` or is listed here are
    /// included in the generated `channels!` macro call. This must match
    /// the `features = [...]` list in the app's Cargo.toml dependency.
    ///
    /// ```toml
    /// [actors]
    /// name = "pool"
    /// blox = "pool-blox"
    /// features = ["dynamic"]
    /// ```
    #[serde(default)]
    pub features: Vec<String>,
}

/// A value in `[actors.inject]` — where a constructor param's handle comes from.
///
/// When `source = "factory"`, `crate` and `function` identify the factory
/// function to inject as a function pointer.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
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
    /// Index for `source = "self_secondary"` — which additional
    /// channel ref to use from a multi-mailbox actor's `channels!` call.
    /// Defaults to 1 (the second channel).
    #[serde(default)]
    pub index: Option<usize>,
}

/// A `[[supervision]]` entry — one supervisor group.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SupervisionConfig {
    /// Supervisor crate/spec name (e.g. `"bloxide-supervisor"`).
    pub supervisor: String,
    /// Group shutdown strategy: `"when_any_done"` (shut the group down when any
    /// child ends) or `"when_all_done"` (wait for all children). Maps directly
    /// to `GroupShutdown::WhenAnyDone` / `GroupShutdown::WhenAllDone`.
    pub strategy: String,
    /// Child actor names managed by this supervisor.
    pub children: Vec<String>,
    /// Per-child policies.
    ///
    /// ```toml
    /// [supervision.policies]
    /// ping = { stop = true }
    /// pong = { stop = true }
    /// ```
    #[serde(default)]
    pub policies: BTreeMap<String, ChildPolicyConfig>,
    /// Control-plane channel capacities. All optional — defaults are computed
    /// from the child count (see `SupervisionCapacities`).
    #[serde(default)]
    pub capacities: SupervisionCapacities,
    /// Watchdog configuration. When present, codegen emits a timer-based driver
    /// that sends `ChildCtrl::WatchdogTick` to the supervisor's control channel
    /// at the configured interval.
    #[serde(default)]
    pub watchdog: Option<WatchdogConfig>,
}

/// A value in `[supervision.policies]` — restart or stop policy for a child.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ChildPolicyConfig {
    /// Restart policy with max restart count.
    pub restart: Option<RestartPolicy>,
    /// When `true`, the supervisor stops the entire group when this child terminates.
    pub stop: Option<bool>,
}

/// Restart policy parameters.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RestartPolicy {
    /// Maximum **consecutive** restarts before the supervisor gives up on the
    /// child (marks it terminal and evaluates group shutdown). The counter
    /// resets when the child proves sustained uptime by answering a health
    /// Ping after a Started. Must be >= 1.
    pub max: u32,
}

/// `[supervision.capacities]` — control-plane channel capacities.
///
/// These channels carry the group's supervision control plane; they are
/// sized to make "full" a bug rather than routine backpressure. (The
/// confirm-before-record protocol still handles a full channel correctly —
/// commands are queued and retried — but sizes should make that rare.)
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct SupervisionCapacities {
    /// Supervisor notify channel capacity (`ChildLifecycleEvent` from every
    /// child). Default: `max(32, 2 × child count)`.
    pub notify: Option<usize>,
    /// Supervisor control channel capacity (registrations, watchdog ticks).
    /// Default: 16.
    pub control: Option<usize>,
    /// Per-child lifecycle channel capacity. Default: 4.
    pub lifecycle: Option<usize>,
}

/// `[supervision.watchdog]` — health-check driver configuration.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct WatchdogConfig {
    /// Interval between health-check rounds in milliseconds.
    pub interval_ms: u64,
    /// Consecutive unanswered Pings before a child is declared rogue.
    /// Default: 2.
    #[serde(default = "default_max_misses")]
    pub max_misses: u8,
}

fn default_max_misses() -> u8 {
    2
}
