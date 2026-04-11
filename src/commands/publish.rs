use crate::commands::CommandCtx;
use crate::config::Config;
use crate::pr_store::PrStore;
use clap::Args;
use console::style;
use eyre::{Context, Result};
use serde::Deserialize;

#[derive(Args, Debug)]
pub struct PublishArgs {
    /// Revset of commits whose PRs to publish
    #[arg(short = 'r', long = "revset")]
    pub revset: Option<String>,
    /// Convert ready-for-review PRs back to draft
    #[arg(long)]
    pub undo: bool,
}

#[derive(Deserialize, Debug)]
struct JjLogCommit {
    change_id: String,
    description: String,
}

pub fn run(args: &PublishArgs, config: &Config, pr_store: &dyn PrStore) -> Result<()> {
    let ctx = CommandCtx::new()?;
    run_with_ctx(args, config, pr_store, &ctx)
}

pub fn run_with_ctx(
    args: &PublishArgs,
    config: &Config,
    pr_store: &dyn PrStore,
    ctx: &CommandCtx,
) -> Result<()> {
    let _ = pr_store; // used via config.prs
    let jj = &ctx.jj;
    let upstream = ctx.require_upstream(config)?;
    let gh = ctx.gh(config)?;

    let revset = args.revset.as_deref().unwrap_or("@");

    let output_str = jj
        .log_reversed(revset, "json(self) ++ \"\\n\"")
        .context("jj log failed")?;

    let commits: Vec<JjLogCommit> = output_str
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            serde_json::from_str(l)
                .with_context(|| format!("Error parsing jj log JSON output. Line: {}", l))
        })
        .collect::<Result<_>>()?;

    if commits.is_empty() {
        eprintln!("No commits matched revset '{}'", revset);
        return Ok(());
    }

    // Phase 1: separate commits into no-PR (skip) and has-PR, then query draft status in batch.
    let mut no_pr_commits: Vec<&JjLogCommit> = Vec::new();
    let mut has_pr_commits: Vec<(&JjLogCommit, u32)> = Vec::new();

    for commit in &commits {
        match config.prs.get(&commit.change_id) {
            Some(&pr_num) => has_pr_commits.push((commit, pr_num)),
            None => no_pr_commits.push(commit),
        }
    }

    let pr_nums: Vec<u32> = has_pr_commits.iter().map(|(_, n)| *n).collect();
    let draft_info = gh
        .pr_node_ids_and_draft_status(&upstream, &pr_nums)
        .context("Failed to fetch PR draft status")?;

    // Phase 2: determine which PRs need changing, collect node IDs.
    let mut to_change_node_ids: Vec<String> = Vec::new();
    let mut already_in_state: Vec<(&JjLogCommit, u32)> = Vec::new();

    for &(commit, pr_num) in &has_pr_commits {
        if let Some((node_id, is_draft)) = draft_info.get(&pr_num) {
            let already = (*is_draft && args.undo) || (!is_draft && !args.undo);
            if already {
                already_in_state.push((commit, pr_num));
            } else {
                to_change_node_ids.push(node_id.clone());
            }
        }
    }

    gh.pr_ready_batch(&to_change_node_ids, args.undo)
        .context("Failed to update PR state")?;

    // Phase 3: print results.
    for commit in &no_pr_commits {
        eprintln!(
            "{} {} — no PR associated, skipping",
            style("~").yellow(),
            &commit.change_id[..12],
        );
    }

    for (commit, pr_num) in &already_in_state {
        let title = commit.description.lines().next().unwrap_or("").trim();
        let already = if args.undo {
            "already unpublished"
        } else {
            "already published"
        };
        eprintln!(
            "{} #{} {} - {}",
            style("~").yellow(),
            pr_num,
            style(title).dim(),
            style(already).dim()
        );
    }

    let published = to_change_node_ids.len();
    for &(commit, pr_num) in &has_pr_commits {
        if draft_info
            .get(&pr_num)
            .map(|(_, is_draft)| {
                let already = (*is_draft && args.undo) || (!is_draft && !args.undo);
                !already
            })
            .unwrap_or(false)
        {
            let title = commit.description.lines().next().unwrap_or("").trim();
            let status_style = if args.undo {
                style("unpublished").yellow()
            } else {
                style("published").green()
            };
            eprintln!(
                "{} #{} {} - {}",
                style("✓").green().bold(),
                pr_num,
                style(title).dim(),
                status_style
            );
        }
    }

    if published > 0 || !no_pr_commits.is_empty() || !already_in_state.is_empty() {
        eprintln!();
    }

    match (published, args.undo) {
        (0, false) => eprintln!("Nothing to publish."),
        (0, true) => eprintln!("Nothing to unpublish."),
        (1, false) => eprintln!("Published 1 PR."),
        (n, false) => eprintln!("Published {} PRs.", n),
        (1, true) => eprintln!("Unpublished 1 PR."),
        (n, true) => eprintln!("Unpublished {} PRs.", n),
    }

    Ok(())
}
