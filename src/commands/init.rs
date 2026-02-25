use crate::config;
use crate::jj::Jj;
use crate::repo;
use anyhow::{anyhow, Context, Result};
use clap::Args;
use std::io::{self, Write};

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Reconfigure upstream even if it's already configured
    #[arg(long)]
    force: bool,
}

pub fn run(args: &InitArgs) -> Result<()> {
    let repo_root = repo::find_root()
        .ok_or_else(|| anyhow!("Not a jujutsu repository (or any of the parent directories): .jj"))?;

    let jj = Jj::new(repo_root.clone());

    let config = config::load(&repo_root).context("Error loading config")?;

    if config.upstream.is_some() && config.origin.is_some() {
        if !args.force {
            println!("jellycat upstream and origin are already configured. Use --force to reconfigure.");
            return Ok(());
        } else {
            println!("Reconfiguring jellycat due to --force flag.");
        }
    }

    if config.upstream.is_none() || args.force {
        println!("Configuring jellycat upstream.");
        print!("Please enter the upstream repo (e.g., owner/repo): ");
        io::stdout().flush().unwrap();
        let mut upstream_input = String::new();
        io::stdin()
            .read_line(&mut upstream_input)
            .context("Failed to read line")?;
        let mut owner_repo = upstream_input.trim().to_string();

        if owner_repo.is_empty() {
            return Err(anyhow!("Upstream repo cannot be empty."));
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
            return Err(anyhow!(
                "Error: Invalid owner/repo format. Expected 'owner/repo', but got '{}'",
                owner_repo
            ));
        }

        config::save(&repo_root, "jellycat.upstream", &owner_repo)
            .context("Error saving config")?;

        println!("Upstream repo configured successfully as: {}", owner_repo);
    }

    if config.origin.is_none() || args.force {
        let remotes_str = jj.git_remote_list().context("Error listing git remotes")?;

        let remotes: Vec<(String, String)> = remotes_str
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let name = parts.next()?.to_string();
                let url = parts.next()?.to_string();
                Some((name, url))
            })
            .collect();

        if remotes.is_empty() {
            return Err(anyhow!(
                "No git remotes found. Please add a remote using 'jj git remote add'."
            ));
        }

        println!("\nAvailable git remotes:");
        for (i, (name, url)) in remotes.iter().enumerate() {
            println!("{}: {} ({})", i + 1, name, url);
        }

        print!("Select a remote to use as origin [1-{}]: ", remotes.len());
        io::stdout().flush().unwrap();

        let mut selection = String::new();
        io::stdin()
            .read_line(&mut selection)
            .context("Failed to read line")?;
        let selection: usize = match selection.trim().parse() {
            Ok(n) if n > 0 && n <= remotes.len() => n,
            _ => {
                return Err(anyhow!("Invalid selection."));
            }
        };

        let (selected_remote_name, _) = &remotes[selection - 1];

        config::save(&repo_root, "jellycat.origin", selected_remote_name)
            .context("Error saving config")?;

        println!(
            "Origin remote configured successfully as: {}",
            selected_remote_name
        );
    }

    Ok(())
}
