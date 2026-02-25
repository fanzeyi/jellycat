use clap::Parser;
use anyhow::Context;

mod commands;
mod config;
mod jj;
mod repo;

use commands::Commands;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init(args) => {
            return commands::init::run(args);
        }
        Commands::Link(args) => {
            return commands::link::run(args);
        }
        Commands::Unlink(args) => {
            return commands::unlink::run(args);
        }
        _ => {} // Continue for commands that need config
    }

    let repo_root = repo::find_root()
        .ok_or_else(|| anyhow::anyhow!("Not a jujutsu repository (or any of the parent directories): .jj"))?;

    let config = config::load(&repo_root).context("Error loading config")?;

    if config.upstream.is_none() {
        return Err(anyhow::anyhow!("jellycat upstream not configured. Please run `jellycat init`."));
    }

    match &cli.command {
        Commands::Submit(args) => commands::submit::run(args, &config),
        Commands::Status(args) => commands::status::run(args, &config),
        Commands::Init(_) | Commands::Link(_) | Commands::Unlink(_) => unreachable!(),
    }
}
