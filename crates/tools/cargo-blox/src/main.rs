// Copyright 2025 Bloxide, all rights reserved
//! Cargo subcommand for Bloxide — generate, build, and manage actor projects.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod build;
mod check;
mod ci;
mod forward;
mod generate;
mod lint;
mod list_cmd;
mod message_cmd;
mod new;
mod new_actions;
mod new_all;
mod new_binary;
mod new_messages;
mod run;
mod state;
mod test;
mod toml_helpers;
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
        /// Actions crate dependency name (e.g. foo-actions)
        #[arg(long)]
        actions: Option<String>,
    },
    /// Scaffold a new actions crate
    NewActions { name: String },
    /// Scaffold a new messages crate
    NewMessages { name: String },
    /// Scaffold a new binary (wiring) crate
    NewBinary {
        name: String,
        /// Runtime to target (tokio or embassy)
        #[arg(long, default_value = "tokio")]
        runtime: String,
    },
    /// Scaffold all layers (messages, actions, blox, binary)
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
        terminal: bool,
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
                actions,
            } => new::new_blox(&name, messages.as_deref(), actions.as_deref()),
            BloxSubcommand::NewActions { name } => new_actions::new_actions(&name),
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
                terminal,
                error,
            } => state::add_state(
                &blox_name,
                &state_name,
                parent.as_deref(),
                composite,
                terminal,
                error,
            ),
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
        },
    }
}
