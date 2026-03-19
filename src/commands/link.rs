use crate::config;
use crate::repo;
use anyhow::{Context, Result, anyhow};
use clap::Args;

#[derive(Args, Debug)]
pub struct LinkArgs {
    /// The revset to link a PR to (must resolve to a single commit)
    #[arg(short = 'r', long = "revset", default_value = "@")]
    pub revset: String,

    /// The PR number to link
    pub pr_number: u32,

    /// Overwrite existing PR link
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: &LinkArgs) -> Result<()> {
    let repo_root = repo::find_root().ok_or_else(|| {
        anyhow!("Not a jujutsu repository (or any of the parent directories): .jj")
    })?;

    // 1. Get the commit.
    let commit =
        repo::get_single_commit(&repo_root, &args.revset).context("Failed to get commit")?;

    // 2. Check for existing PR link in config.
    let cfg = config::load(&repo_root)?;
    if let Some(&existing_pr) = cfg.prs.get(&commit.change_id) {
        if !args.force && existing_pr == args.pr_number {
            println!(
                "PR #{} is already linked to changeset {}.",
                args.pr_number, commit.change_id
            );
            return Ok(());
        } else if !args.force {
            return Err(anyhow!(
                "Commit is already linked to PR #{}. Use --force to overwrite.",
                existing_pr
            ));
        }
    }

    // 3. Save PR mapping to config.
    let key = format!("jellycat.prs.{}", commit.change_id);
    config::save(&repo_root, &key, &args.pr_number.to_string())?;

    println!(
        "Linked PR #{} to change {}",
        args.pr_number, commit.change_id
    );

    Ok(())
}
