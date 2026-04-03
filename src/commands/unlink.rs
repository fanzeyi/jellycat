use crate::config::Config;
use crate::pr_store::PrStore;
use crate::repo;
use clap::Args;
use eyre::{Result, eyre};

#[derive(Args, Debug)]
pub struct UnlinkArgs {
    /// The revset to unlink a PR from (must resolve to a single commit)
    #[arg(short = 'r', long = "revset")]
    pub revset: Option<String>,

    /// The PR number to unlink
    #[arg(short = 'p', long = "pr")]
    pub pr_number: Option<u32>,
}

pub fn run(args: &UnlinkArgs, config: &Config, pr_store: &dyn PrStore) -> Result<()> {
    if args.revset.is_none() && args.pr_number.is_none() {
        return Err(eyre!("Either --revset or --pr must be provided."));
    }

    let repo_root = repo::find_root()
        .ok_or_else(|| eyre!("Not a jujutsu repository (or any of the parent directories): .jj"))?;

    // If --pr is given without --revset, find the change_id that maps to this PR.
    if let (None, Some(pr_number)) = (&args.revset, args.pr_number) {
        let change_id = pr_store.find_by_pr(pr_number)?;

        match change_id {
            Some(cid) => {
                pr_store.unset(&cid)?;
                println!("Unlinked PR #{} from changeset {}", pr_number, cid);
            }
            None => {
                eprintln!("Warning: No changeset found linked to PR #{}.", pr_number);
            }
        }
        return Ok(());
    }

    // --revset is provided (possibly with --pr)
    let revset = args.revset.as_deref().unwrap();
    let commit = repo::get_single_commit(&repo_root, revset)?;
    let existing_pr = config.prs.get(&commit.change_id).copied();

    match (existing_pr, args.pr_number) {
        (None, _) => {
            eprintln!(
                "Warning: No PR found linked to changeset {}.",
                commit.change_id
            );
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

    pr_store.unset(&commit.change_id)?;

    let pr = args.pr_number.or(existing_pr).unwrap();
    println!("Unlinked PR #{} from changeset {}", pr, commit.change_id);

    Ok(())
}
