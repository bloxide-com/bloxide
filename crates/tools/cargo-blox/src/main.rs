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
mod new;
mod run;
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
        },
    }
}
