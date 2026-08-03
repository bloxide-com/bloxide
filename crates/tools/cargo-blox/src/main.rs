// Copyright 2025 Bloxide, all rights reserved
//! Cargo subcommand for Bloxide — generate, build, and manage actor projects.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod build;
mod check;
mod ci;
mod context_cmd;
mod entry_exit_cmd;
mod exit;
mod forward;
mod generate;
mod init;
mod lint;
mod list_cmd;
mod message_cmd;
mod new;
mod new_all;
mod new_binary;
mod new_context;
mod new_impl;
mod new_messages;
mod run;
mod state;
mod system_cmd;
mod test;
mod toml_helpers;
mod transition_cmd;
mod utils;
mod verify;
mod viz;
mod watch;
mod wire;

#[derive(Parser)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
struct CargoCli {
    #[command(subcommand)]
    command: BloxCommand,
}

#[derive(Subcommand)]
enum BloxCommand {
    #[command(name = "blox")]
    Blox(BloxArgs),
}

#[derive(Parser)]
struct BloxArgs {
    #[command(subcommand)]
    command: BloxSubcommand,
}

#[derive(Subcommand)]
enum BloxSubcommand {
    /// Generate code from all blox.toml files in workspace
    Generate {
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Generate, then build
    Build {
        #[command(flatten)]
        cargo: clap_cargo::Features,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Generate, then check
    Check {
        #[command(flatten)]
        cargo: clap_cargo::Features,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Generate, then test
    Test {
        #[command(flatten)]
        cargo: clap_cargo::Features,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Generate, then run
    Run {
        #[command(flatten)]
        cargo: clap_cargo::Features,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Watch and regenerate on changes
    Watch {
        #[command(flatten)]
        cargo: clap_cargo::Features,
    },
    /// Scaffold a new blox crate
    New {
        name: String,
        /// Messages crate dependency name (e.g. foo-messages)
        #[arg(long)]
        messages: Option<String>,
        /// Context crate dependency name (e.g. blox-ctx-foo)
        #[arg(long)]
        context: Option<String>,
    },
    /// Scaffold a new context crate (action functions)
    NewContext { name: String },
    /// Scaffold a new impl crate for a blox
    NewImpl {
        name: String,
        /// Blox crate name (e.g. pool)
        #[arg(long)]
        blox: String,
    },
    /// Scaffold a new messages crate
    NewMessages { name: String },
    /// Scaffold a new binary (wiring) crate
    NewBinary {
        name: String,
        /// Runtime to target (tokio or embassy)
        #[arg(long, default_value = "tokio")]
        runtime: String,
    },
    /// Scaffold all layers (messages, context, blox, binary)
    NewAll {
        name: String,
        /// Runtime to target (tokio or embassy)
        #[arg(long, default_value = "tokio")]
        runtime: String,
    },
    /// Run spec-to-code lint checks
    Lint,
    /// Run full CI feature matrix
    Ci,
    /// Generate a binary main.rs from a system.toml wiring manifest
    Wire {
        /// Path to system.toml (default: workspace root)
        #[arg(long)]
        system: Option<PathBuf>,
        /// Output path for main.rs (default: <system.toml dir>/src/main.rs)
        #[arg(long)]
        output: Option<PathBuf>,
        /// After generating main.rs, run the generated binary crate
        #[arg(long)]
        run: bool,
    },
    /// Verify round-trip: blox.toml → codegen → viz-export → JSON → compare
    Verify {
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Add a state to a blox topology
    AddState {
        blox_name: String,
        state_name: String,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        composite: bool,
        #[arg(long)]
        error: bool,
    },
    /// Remove a state from a blox topology
    RemoveState {
        blox_name: String,
        state_name: String,
    },
    /// List states in a blox
    ListStates {
        blox_name: String,
        #[arg(long)]
        json: bool,
    },
    /// List transitions in a blox
    ListTransitions {
        blox_name: String,
        #[arg(long)]
        json: bool,
    },
    /// List message variants in a messages crate
    ListMessages {
        crate_name: String,
        #[arg(long)]
        json: bool,
    },
    /// List all blox crates in the workspace
    ListBloxes {
        #[arg(long)]
        json: bool,
    },
    /// Add a message variant to a messages crate
    AddMessage {
        crate_name: String,
        variant_name: String,
        #[arg(trailing_var_arg = true)]
        fields: Vec<String>,
    },
    /// Remove a message variant from a messages crate
    RemoveMessage {
        crate_name: String,
        variant_name: String,
    },
    /// Add a transition to a blox topology
    AddTransition {
        blox_name: String,
        #[arg(long)]
        state: String,
        #[arg(long)]
        event: String,
        #[arg(long)]
        target: String,
        #[arg(long)]
        action: Vec<String>,
        #[arg(long)]
        guard: Vec<String>,
        #[arg(long)]
        feature: Option<String>,
        #[arg(long)]
        if_not_exists: bool,
    },
    /// Remove a transition from a blox topology
    RemoveTransition {
        blox_name: String,
        #[arg(long)]
        state: String,
        #[arg(long)]
        event: String,
    },
    /// Add an entry hook to a blox state
    AddEntry {
        blox_name: String,
        #[arg(long)]
        state: String,
        #[arg(long)]
        action: Vec<String>,
        #[arg(long)]
        feature: Option<String>,
        #[arg(long)]
        if_not_exists: bool,
    },
    /// Remove an entry hook from a blox state
    RemoveEntry {
        blox_name: String,
        #[arg(long)]
        state: String,
    },
    /// Add an exit hook to a blox state
    AddExit {
        blox_name: String,
        #[arg(long)]
        state: String,
        #[arg(long)]
        action: Vec<String>,
        #[arg(long)]
        feature: Option<String>,
        #[arg(long)]
        if_not_exists: bool,
    },
    /// Remove an exit hook from a blox state
    RemoveExit {
        blox_name: String,
        #[arg(long)]
        state: String,
    },
    /// Add an actor to a system.toml
    AddActor {
        app_name: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        blox: String,
        #[arg(long)]
        impl_crate: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        feature: Vec<String>,
        #[arg(long)]
        if_not_exists: bool,
    },
    /// Remove an actor from a system.toml (also cleans supervision refs)
    RemoveActor {
        app_name: String,
        #[arg(long)]
        name: String,
    },
    /// Add a supervision section to a system.toml
    AddSupervision {
        app_name: String,
        #[arg(long)]
        supervisor: String,
        #[arg(long)]
        strategy: String,
        #[arg(long)]
        child: Vec<String>,
        #[arg(long)]
        if_not_exists: bool,
    },
    /// Remove a supervision section from a system.toml
    RemoveSupervision {
        app_name: String,
        #[arg(long)]
        supervisor: String,
    },
    /// Set a child policy in a system.toml supervision section
    SetPolicy {
        app_name: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        restart_max: Option<u32>,
        #[arg(long)]
        stop: bool,
    },
    /// Bootstrap a new bloxide workspace
    Init {
        /// Target directory to create (e.g. ./my-app)
        dir: String,
        /// Runtime for the hello-world app: tokio or embassy
        #[arg(long, default_value = "tokio")]
        runtime: String,
    },
    /// Launch the visualizer (or export specs as JSON)
    Viz {
        /// Export blox specs as JSON to this directory instead of launching
        #[arg(long)]
        export: Option<std::path::PathBuf>,
        /// Server port (default 8080)
        #[arg(long, default_value = "8080")]
        port: u16,
        /// Open the browser after launching
        #[arg(long)]
        open: bool,
    },
    /// Add a constructor injection to an actor in a system.toml
    AddInjection {
        app_name: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        field: String,
        #[arg(long)]
        from: String,
    },
    /// Add a [[context.uses]] entry to a blox
    AddUse {
        blox_name: String,
        #[arg(long)]
        crate_name: String,
        #[arg(long)]
        field: String,
        #[arg(long)]
        field_type: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        feature: Option<String>,
        #[arg(long)]
        if_not_exists: bool,
    },
    /// Remove a [[context.uses]] entry from a blox
    RemoveUse {
        blox_name: String,
        #[arg(long)]
        field: String,
    },
    /// Add a [[context.fields]] state field to a blox
    AddField {
        blox_name: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        ty: String,
        #[arg(long)]
        default: Option<String>,
        #[arg(long)]
        if_not_exists: bool,
    },
    /// Remove a context field from a blox (fields, uses, or uses sub-fields)
    RemoveField {
        blox_name: String,
        #[arg(long)]
        name: String,
    },
    /// Add a [[context.actions]] entry to a blox
    AddAction {
        blox_name: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        field: Vec<String>,
        #[arg(long)]
        crate_name: Option<String>,
        #[arg(long)]
        module: Option<String>,
        #[arg(long)]
        fn_name: Option<String>,
        #[arg(long)]
        event_payload: Option<String>,
        #[arg(long)]
        impl_required: bool,
        #[arg(long)]
        feature: Option<String>,
        #[arg(long)]
        if_not_exists: bool,
    },
    /// Remove a [[context.actions]] entry from a blox
    RemoveAction {
        blox_name: String,
        #[arg(long)]
        name: String,
    },
}

fn main() {
    if let Err(err) = dispatch() {
        exit::exit_process(err);
    }
}

fn dispatch() -> anyhow::Result<()> {
    let cli = CargoCli::parse();
    match cli.command {
        BloxCommand::Blox(args) => match args.command {
            BloxSubcommand::Generate { workspace } => generate::generate(workspace),
            BloxSubcommand::Build { cargo, args } => build::build(cargo, args),
            BloxSubcommand::Check { cargo, args } => check::check(cargo, args),
            BloxSubcommand::Test { cargo, args } => test::test(cargo, args),
            BloxSubcommand::Run { cargo, args } => run::run(cargo, args),
            BloxSubcommand::Watch { cargo } => watch::watch(cargo),
            BloxSubcommand::New {
                name,
                messages,
                context,
            } => new::new_blox(&name, messages.as_deref(), context.as_deref()),
            BloxSubcommand::NewContext { name } => new_context::new_context(&name),
            BloxSubcommand::NewImpl { name, blox } => new_impl::new_impl(&name, &blox),
            BloxSubcommand::NewMessages { name } => new_messages::new_messages(&name),
            BloxSubcommand::NewBinary { name, runtime } => new_binary::new_binary(&name, &runtime),
            BloxSubcommand::NewAll { name, runtime } => new_all::new_all(&name, &runtime),
            BloxSubcommand::Lint => lint::lint(),
            BloxSubcommand::Ci => ci::ci(),
            BloxSubcommand::Verify { workspace } => verify::verify(workspace),
            BloxSubcommand::Wire {
                system,
                output,
                run,
            } => wire::wire(system, output, run),
            BloxSubcommand::AddState {
                blox_name,
                state_name,
                parent,
                composite,
                error,
            } => state::add_state(&blox_name, &state_name, parent.as_deref(), composite, error),
            BloxSubcommand::RemoveState {
                blox_name,
                state_name,
            } => state::remove_state(&blox_name, &state_name),
            BloxSubcommand::ListStates { blox_name, json } => {
                list_cmd::list_states(&blox_name, json)
            }
            BloxSubcommand::ListTransitions { blox_name, json } => {
                list_cmd::list_transitions(&blox_name, json)
            }
            BloxSubcommand::ListMessages { crate_name, json } => {
                list_cmd::list_messages(&crate_name, json)
            }
            BloxSubcommand::ListBloxes { json } => list_cmd::list_bloxes(json),
            BloxSubcommand::AddMessage {
                crate_name,
                variant_name,
                fields,
            } => {
                let parsed_fields: Vec<(String, String)> = fields
                    .iter()
                    .filter_map(|s| {
                        let parts: Vec<&str> = s.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            Some((parts[0].to_string(), parts[1].to_string()))
                        } else {
                            eprintln!(
                                "warning: skipping invalid field spec '{}' (expected name:ty)",
                                s
                            );
                            None
                        }
                    })
                    .collect();
                message_cmd::add_message(&crate_name, &variant_name, parsed_fields)
            }
            BloxSubcommand::RemoveMessage {
                crate_name,
                variant_name,
            } => message_cmd::remove_message(&crate_name, &variant_name),
            BloxSubcommand::AddTransition {
                blox_name,
                state,
                event,
                target,
                action,
                guard,
                feature,
                if_not_exists,
            } => transition_cmd::add_transition(
                &blox_name,
                &state,
                &event,
                &target,
                action,
                guard,
                feature.as_deref(),
                if_not_exists,
            ),
            BloxSubcommand::RemoveTransition {
                blox_name,
                state,
                event,
            } => transition_cmd::remove_transition(&blox_name, &state, &event),
            BloxSubcommand::AddEntry {
                blox_name,
                state,
                action,
                feature,
                if_not_exists,
            } => entry_exit_cmd::add_entry(
                &blox_name,
                &state,
                action,
                feature.as_deref(),
                if_not_exists,
            ),
            BloxSubcommand::RemoveEntry { blox_name, state } => {
                entry_exit_cmd::remove_entry(&blox_name, &state)
            }
            BloxSubcommand::AddExit {
                blox_name,
                state,
                action,
                feature,
                if_not_exists,
            } => entry_exit_cmd::add_exit(
                &blox_name,
                &state,
                action,
                feature.as_deref(),
                if_not_exists,
            ),
            BloxSubcommand::RemoveExit { blox_name, state } => {
                entry_exit_cmd::remove_exit(&blox_name, &state)
            }
            BloxSubcommand::AddActor {
                app_name,
                name,
                blox,
                impl_crate,
                kind,
                feature,
                if_not_exists,
            } => system_cmd::add_actor(
                &app_name,
                &name,
                &blox,
                impl_crate.as_deref(),
                kind.as_deref(),
                feature,
                if_not_exists,
            ),
            BloxSubcommand::RemoveActor { app_name, name } => {
                system_cmd::remove_actor(&app_name, &name)
            }
            BloxSubcommand::AddSupervision {
                app_name,
                supervisor,
                strategy,
                child,
                if_not_exists,
            } => {
                system_cmd::add_supervision(&app_name, &supervisor, &strategy, child, if_not_exists)
            }
            BloxSubcommand::RemoveSupervision {
                app_name,
                supervisor,
            } => system_cmd::remove_supervision(&app_name, &supervisor),
            BloxSubcommand::SetPolicy {
                app_name,
                actor,
                restart_max,
                stop,
            } => system_cmd::set_policy(&app_name, &actor, restart_max, stop),
            BloxSubcommand::Init { dir, runtime } => init::init(&dir, &runtime),
            BloxSubcommand::Viz { export, port, open } => viz::viz(export, port, open),
            BloxSubcommand::AddInjection {
                app_name,
                actor,
                field,
                from,
            } => system_cmd::add_injection(&app_name, &actor, &field, &from),
            BloxSubcommand::AddUse {
                blox_name,
                crate_name,
                field,
                field_type,
                role,
                feature,
                if_not_exists,
            } => context_cmd::add_use(
                &blox_name,
                &crate_name,
                &field,
                &field_type,
                &role,
                feature.as_deref(),
                if_not_exists,
            ),
            BloxSubcommand::RemoveUse { blox_name, field } => {
                context_cmd::remove_use(&blox_name, &field)
            }
            BloxSubcommand::AddField {
                blox_name,
                name,
                ty,
                default,
                if_not_exists,
            } => context_cmd::add_field(&blox_name, &name, &ty, default.as_deref(), if_not_exists),
            BloxSubcommand::RemoveField { blox_name, name } => {
                context_cmd::remove_field(&blox_name, &name)
            }
            BloxSubcommand::AddAction {
                blox_name,
                name,
                field,
                crate_name,
                module,
                fn_name,
                event_payload,
                impl_required,
                feature,
                if_not_exists,
            } => context_cmd::add_action(
                &blox_name,
                &name,
                field,
                crate_name.as_deref(),
                module.as_deref(),
                fn_name.as_deref(),
                event_payload.as_deref(),
                impl_required,
                feature.as_deref(),
                if_not_exists,
            ),
            BloxSubcommand::RemoveAction { blox_name, name } => {
                context_cmd::remove_action(&blox_name, &name)
            }
        },
    }
}
