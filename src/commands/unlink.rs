use crate::config;
use crate::jj::Jj;
use crate::repo;
use anyhow::{Result, anyhow};
use clap::Args;

#[derive(Args, Debug)]
pub struct UnlinkArgs {
    /// The revset to unlink a PR from (must resolve to a single commit)
    #[arg(short = 'r', long = "revset")]
    pub revset: Option<String>,

    /// The PR number to unlink
    #[arg(short = 'p', long = "pr")]
    pub pr_number: Option<u32>,
}

pub fn run(args: &UnlinkArgs) -> Result<()> {
    if args.revset.is_none() && args.pr_number.is_none() {
        return Err(anyhow!(
            "Either --revset or --pr must be provided."
        ));
    }

    let repo_root = repo::find_root().ok_or_else(|| {
        anyhow!("Not a jujutsu repository (or any of the parent directories): .jj")
    })?;

    let jj = Jj::new(repo_root.clone());
    let cfg = config::load(&repo_root)?;

    // If --pr is given without --revset, find the change_id that maps to this PR.
    if let (None, Some(pr_number)) = (&args.revset, args.pr_number) {
        let change_id = cfg
            .prs
            .iter()
            .find(|&(_, &pr)| pr == pr_number)
            .map(|(cid, _)| cid.clone());

        match change_id {
            Some(cid) => {
                let key = format!("jellycat.prs.{}", cid);
                jj.config_unset(&key)?;
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
    let existing_pr = cfg.prs.get(&commit.change_id).copied();

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

    let key = format!("jellycat.prs.{}", commit.change_id);
    jj.config_unset(&key)?;

    let pr = args.pr_number.or(existing_pr).unwrap();
    println!(
        "Unlinked PR #{} from changeset {}",
        pr, commit.change_id
    );

    Ok(())
}
