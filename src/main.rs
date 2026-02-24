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

    let cli = Cli::parse();

    match &cli.command {
        Commands::Submit(args) => commands::submit::run(args, &config),
        Commands::Status(args) => commands::status::run(args, &config),
    }
}
