use crate::commands::CommandCtx;
use crate::config::Config;
use crate::gh::Gh;
use crate::pr_store::PrStore;
use crate::repo::{self, JjLogCommit};
use clap::Args;
use eyre::{Context, Result, bail, eyre};
use std::collections::HashSet;

#[derive(Args, Debug)]
pub struct LinkArgs {
    /// The revset to link a PR to (must resolve to a single commit)
    #[arg(short = 'r', long = "revset", default_value = "@")]
    pub revset: String,

    /// The PR number to link
    pub pr_number: Option<u32>,

    /// Overwrite existing PR link
    #[arg(long)]
    pub force: bool,

    /// Auto-link open PRs to local bookmarks that share their head ref name.
    #[arg(long)]
    pub smart: bool,
}

pub fn run(args: &LinkArgs, config: &Config, pr_store: &dyn PrStore) -> Result<()> {
    let ctx = CommandCtx::new()?;

    if args.smart {
        if args.pr_number.is_some() {
            bail!("cannot combine --smart with a PR number");
        }
        let upstream = ctx.require_upstream(config)?.to_string();
        let gh = ctx.gh(config)?;
        return run_smart(args, config, pr_store, &ctx, &gh, &upstream);
    }

    let pr_number = args
        .pr_number
        .ok_or_else(|| eyre!("pr_number is required (or pass --smart)"))?;
    run_single(args, pr_number, config, pr_store, &ctx)
}

fn run_single(
    args: &LinkArgs,
    pr_number: u32,
    config: &Config,
    pr_store: &dyn PrStore,
    ctx: &CommandCtx,
) -> Result<()> {
    // 1. Get the commit.
    let commit =
        repo::get_single_commit(&ctx.repo_root, &args.revset).context("Failed to get commit")?;

    // 2. Check for existing PR link.
    if let Some(&existing_pr) = config.prs.get(&commit.change_id) {
        if !args.force && existing_pr == pr_number {
            println!(
                "PR #{} is already linked to changeset {}.",
                pr_number, commit.change_id
            );
            return Ok(());
        } else if !args.force {
            return Err(eyre!(
                "Commit is already linked to PR #{}. Use --force to overwrite.",
                existing_pr
            ));
        }
    }

    // 3. Save PR mapping via PrStore.
    pr_store.set(&commit.change_id, pr_number)?;

    println!("Linked PR #{} to change {}", pr_number, commit.change_id);

    Ok(())
}

pub fn run_smart(
    args: &LinkArgs,
    config: &Config,
    pr_store: &dyn PrStore,
    ctx: &CommandCtx,
    gh: &Gh,
    upstream: &str,
) -> Result<()> {
    let prs = gh.list_my_open_prs(upstream)?;
    let bookmarks = ctx.jj.bookmark_list(None)?;
    let bookmark_names: HashSet<String> = bookmarks.into_iter().map(|(n, _)| n).collect();

    let mut linked = 0usize;
    let mut skipped = 0usize;
    let mut conflicts = 0usize;

    for (pr_num, head_ref) in prs {
        if !bookmark_names.contains(&head_ref) {
            continue;
        }

        // Resolve the bookmark to a change-id using the ctx's jj client so
        // tests can mock the underlying subprocess.
        let output = ctx.jj.log(&head_ref, "json(self)")?;
        let line = output
            .lines()
            .next()
            .ok_or_else(|| eyre!("No commit found for bookmark {}", head_ref))?;
        let commit: JjLogCommit = serde_json::from_str(line)
            .with_context(|| format!("Failed to parse jj log JSON for bookmark {}", head_ref))?;

        if let Some(&existing_pr) = config.prs.get(&commit.change_id) {
            if existing_pr == pr_num {
                println!(
                    "Skipped PR #{} → {} (already linked)",
                    pr_num, commit.change_id
                );
                skipped += 1;
                continue;
            } else if !args.force {
                println!(
                    "Conflict: bookmark {} (change {}) already linked to PR #{}, not PR #{} (use --force to overwrite)",
                    head_ref, commit.change_id, existing_pr, pr_num
                );
                conflicts += 1;
                continue;
            }
        }

        pr_store.set(&commit.change_id, pr_num)?;
        println!("Linked PR #{} → {}", pr_num, commit.change_id);
        linked += 1;
    }

    if linked == 0 && skipped == 0 && conflicts == 0 {
        println!("No open PRs matched local bookmarks.");
    } else {
        println!(
            "Summary: {} linked, {} skipped, {} conflicts",
            linked, skipped, conflicts
        );
    }

    Ok(())
}
