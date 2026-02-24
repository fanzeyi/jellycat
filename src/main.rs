use clap::Parser;
use std::process::exit;

mod commands;
mod config;
mod repo;

use commands::Commands;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init(args) => {
            commands::init::run(args);
            return;
        }
        Commands::Link(args) => {
            commands::link::run(args);
            return;
        }
        Commands::Unlink(args) => {
            commands::unlink::run(args);
            return;
        }
        _ => {} // Continue for commands that need config
    }

    let repo_root = match repo::find_root() {
        Some(root) => root,
        None => {
            eprintln!("Error: Not a jujutsu repository (or any of the parent directories): .jj");
            exit(1);
        }
    };

    let config = match config::load(&repo_root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            exit(1);
        }
    };

    if config.upstream.is_none() {
        eprintln!("jellycat upstream not configured. Please run `jellycat init`.");
        exit(1);
    }

    match &cli.command {
        Commands::Submit(args) => commands::submit::run(args, &config),
        Commands::Status(args) => commands::status::run(args, &config),
        Commands::Init(_) | Commands::Link(_) | Commands::Unlink(_) => unreachable!(),
    }
}
