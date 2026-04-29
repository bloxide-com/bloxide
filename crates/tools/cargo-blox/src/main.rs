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
mod message_cmd;
mod new;
mod run;
mod state;
mod test;
mod watch;

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
    /// Scaffold a new blox
    New {
        name: String,
    },
    /// Run spec-to-code lint checks
    Lint,
    /// Run full CI feature matrix
    Ci,
    /// Add a state to a blox topology
    AddState {
        blox_name: String,
        state_name: String,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        composite: bool,
    },
    /// Remove a state from a blox topology
    RemoveState {
        blox_name: String,
        state_name: String,
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
            BloxSubcommand::New { name } => new::new_blox(&name),
            BloxSubcommand::Lint => lint::lint(),
            BloxSubcommand::Ci => ci::ci(),
            BloxSubcommand::AddState {
                blox_name,
                state_name,
                parent,
                composite,
            } => state::add_state(&blox_name, &state_name, parent.as_deref(), composite),
            BloxSubcommand::RemoveState {
                blox_name,
                state_name,
            } => state::remove_state(&blox_name, &state_name),
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
                            eprintln!("warning: skipping invalid field spec '{}' (expected name:ty)", s);
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
