use clap::Parser;
use std::process::exit;

mod commands;
mod repo;

use commands::Commands;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn main() {
    if repo::find_root().is_none() {
        eprintln!("Error: Not a jujutsu repository (or any of the parent directories): .jj");
        exit(1);
    }

    let cli = Cli::parse();

    match &cli.command {
        Commands::Submit(args) => commands::submit::run(args),
        Commands::Status(args) => commands::status::run(args),
    }
}
