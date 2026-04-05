use crate::commands::CommandCtx;
use crate::config::Config;
use crate::error::format_error_short;
use crate::gh::Gh;
use crate::pr_store::PrStore;
use crate::repo::{self, JjLogCommit};
use clap::Args;
use eyre::{Context, Result, bail, eyre};
use std::collections::HashSet;
use tracing::instrument;

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

#[derive(Debug)]
enum SmartLinkResult {
    NotFound,
    Skipped,
    Linked,
    Conflict,
    Error(eyre::Report),
}

#[instrument(skip(ctx, pr_store))]
fn link_pr_to_bookmark(
    ctx: &CommandCtx,
    config: &Config,
    pr_store: &dyn PrStore,
    pr_num: u32,
    head_ref: &str,
    force: bool,
) -> SmartLinkResult {
    // Resolve the bookmark to a change-id using the ctx's jj client so
    // tests can mock the underlying subprocess.
    let Ok(output) = ctx
        .jj
        .log(&format!("remote_bookmarks({})", head_ref), "json(self)")
    else {
        println!(
            "Skipped bookmark {} from PR #{} (no commit found)",
            head_ref, pr_num
        );
        return SmartLinkResult::NotFound;
    };
    let Some(line) = output.lines().next() else {
        tracing::debug!(
            "No commit found for bookmark {}: no output from jj log",
            head_ref
        );
        return SmartLinkResult::NotFound;
    };
    let Ok(commit) = serde_json::from_str::<JjLogCommit>(line)
        .with_context(|| format!("Failed to parse jj log JSON for bookmark {}", head_ref))
    else {
        tracing::debug!(
            "Failed to parse jj log JSON for bookmark {}: {}",
            head_ref,
            line
        );
        return SmartLinkResult::NotFound;
    };

    if let Some(&existing_pr) = config.prs.get(&commit.change_id) {
        if existing_pr == pr_num {
            println!(
                "Skipped PR #{} → {} (already linked)",
                pr_num, commit.change_id
            );
            return SmartLinkResult::Skipped;
        } else if !force {
            println!(
                "Conflict: bookmark {} (change {}) already linked to PR #{}, not PR #{} (use --force to overwrite)",
                head_ref, commit.change_id, existing_pr, pr_num
            );
            return SmartLinkResult::Conflict;
        }
    }

    match pr_store.set(&commit.commit_id, pr_num) {
        Ok(()) => {
            println!("Linked PR #{} → {}", pr_num, commit.change_id);
            SmartLinkResult::Linked
        }
        Err(e) => SmartLinkResult::Error(e),
    }
}

#[instrument(skip(pr_store, ctx, gh))]
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

        match link_pr_to_bookmark(ctx, config, pr_store, pr_num, head_ref.as_str(), args.force) {
            SmartLinkResult::Linked => linked += 1,
            SmartLinkResult::Skipped => skipped += 1,
            SmartLinkResult::Conflict => conflicts += 1,
            SmartLinkResult::NotFound => {}
            SmartLinkResult::Error(e) => {
                eprintln!(
                    "Unable to link PR #{} to {}: {}",
                    pr_num,
                    head_ref,
                    format_error_short(&e)
                )
            }
        }
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
