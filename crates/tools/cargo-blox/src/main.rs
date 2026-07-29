// Copyright 2025 Bloxide, all rights reserved
//! Cargo subcommand for Bloxide — generate, build, and manage actor projects.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod build;
mod check;
mod ci;
mod entry_exit_cmd;
mod forward;
mod generate;
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
mod test;
mod toml_helpers;
mod transition_cmd;
mod utils;
mod verify;
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
}

fn main() -> anyhow::Result<()> {
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
        },
    }
}
