use crate::config;
use crate::repo;
use clap::Args;
use std::io::{self, Write};
use std::process::exit;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Reconfigure upstream even if it's already configured
    #[arg(long)]
    force: bool,
}

pub fn run(args: &InitArgs) {
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

    if config.upstream.is_some() {
        if !args.force {
            println!("jellycat upstream is already configured. Use --force to reconfigure.");
            return;
        } else {
            println!("Reconfiguring jellycat upstream due to --force flag.");
        }
    }

    println!("jellycat upstream not configured.");
    print!("Please enter the upstream repo (e.g., owner/repo): ");
    io::stdout().flush().unwrap();
    let mut upstream_input = String::new();
    io::stdin()
        .read_line(&mut upstream_input)
        .expect("Failed to read line");
    let mut owner_repo = upstream_input.trim().to_string();

    if owner_repo.is_empty() {
        eprintln!("Upstream repo cannot be empty. Aborting.");
        exit(1);
    }

    // Remove known prefixes
    if let Some(stripped) = owner_repo.strip_prefix("https://github.com/") {
        owner_repo = stripped.to_string();
    } else if let Some(stripped) = owner_repo.strip_prefix("git@github.com:") {
        owner_repo = stripped.to_string();
    }

    // Remove known suffix
    if let Some(stripped) = owner_repo.strip_suffix(".git") {
        owner_repo = stripped.to_string();
    }

    // Validate the format: should be "owner/repo"
    if !owner_repo.contains('/') || owner_repo.starts_with('/') || owner_repo.ends_with('/') {
        eprintln!("Error: Invalid owner/repo format. Expected 'owner/repo', but got '{}'", owner_repo);
        exit(1);
    }


    if let Err(e) = config::save(&repo_root, "jellycat.upstream", &owner_repo) {
        eprintln!("Error saving config: {}", e);
        exit(1);
    }

    println!("Upstream repo configured successfully as: {}", owner_repo);
}
