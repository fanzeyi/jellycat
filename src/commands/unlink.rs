use crate::config;
use crate::jj::Jj;
use crate::repo;
use anyhow::{Result, anyhow};
use clap::Args;

#[derive(Args, Debug)]
pub struct UnlinkArgs {
    /// The revset to unlink a PR from (must resolve to a single commit)
    #[arg(short = 'r', long = "revset", default_value = "@")]
    pub revset: String,

    /// The PR number to unlink (optional, if not provided, all PRs are unlinked)
    pub pr_number: Option<u32>,
}

pub fn run(args: &UnlinkArgs) -> Result<()> {
    let repo_root = repo::find_root().ok_or_else(|| {
        anyhow!("Not a jujutsu repository (or any of the parent directories): .jj")
    })?;

    let jj = Jj::new(repo_root.clone());

    // 1. Get the commit.
    let commit =
        repo::get_single_commit(&repo_root, &args.revset)?;

    // 2. Check config for existing PR link.
    let cfg = config::load(&repo_root)?;
    let existing_pr = cfg.prs.get(&commit.change_id).copied();

    match (existing_pr, args.pr_number) {
        (None, _) => {
            eprintln!("Warning: No PR found linked to changeset {}.", commit.change_id);
            return Ok(());
        }
        (Some(existing), Some(requested)) if existing != requested => {
            eprintln!(
                "Warning: PR #{} not found linked to changeset {} (linked to PR #{}).",
                requested, commit.change_id, existing
            );
            return Ok(());
        }
        _ => {}
    }

    // 3. Remove PR mapping from config.
    let key = format!("jellycat.prs.{}", commit.change_id);
    jj.config_unset(&key)?;

    if let Some(pr_number) = args.pr_number {
        println!(
            "Unlinked PR #{} from changeset {}",
            pr_number, commit.change_id
        );
    } else {
        let pr = existing_pr.unwrap();
        println!(
            "Unlinked PR #{} from changeset {}",
            pr, commit.change_id
        );
    }

    Ok(())
}
