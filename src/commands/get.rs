use crate::commands::CommandCtx;
use crate::config::Config;
use crate::pr_store::PrStore;
use crate::repo;
use clap::Args;
use console::style;
use eyre::{Result, eyre};

#[derive(Args, Debug)]
pub struct GetArgs {
    /// PR number to fetch
    pub pr_number: u32,

    /// Check out the fetched branch (jj new)
    #[arg(long, alias = "update")]
    pub checkout: bool,

    /// Rebase the fetched branch onto the current commit
    #[arg(long)]
    pub rebase: bool,
}

pub fn run(args: &GetArgs, config: &Config, pr_store: &dyn PrStore) -> Result<()> {
    let ctx = CommandCtx::new()?;
    run_with_ctx(args, config, pr_store, &ctx)
}

/// Internal entry point used by tests with a mock runner.
pub fn run_with_ctx(
    args: &GetArgs,
    config: &Config,
    pr_store: &dyn PrStore,
    ctx: &CommandCtx,
) -> Result<()> {
    let jj = &ctx.jj;
    let upstream_repo = ctx.require_upstream(config)?;

    let remote_name = if let Some(ref name) = config.upstream {
        name.clone()
    } else {
        jj.find_upstream_remote(upstream_repo)?
    };

    let local_branch = format!("pr-{}", args.pr_number);
    let refspec = format!(
        "refs/pull/{}/head:refs/heads/{}",
        args.pr_number, local_branch
    );

    eprintln!(
        "Fetching PR #{} from remote '{}'...",
        args.pr_number, remote_name
    );

    // Find the remote URL from jj git remote list
    let remote_output = jj.git_remote_list()?;
    let remote_url = remote_output
        .lines()
        .find(|line| line.starts_with(&format!("{} ", remote_name)))
        .and_then(|line| line.split_once(' '))
        .map(|(_, url)| url.trim())
        .ok_or_else(|| eyre!("Could not find URL for remote '{}'", remote_name))?
        .to_string();

    // Run git fetch with the remote URL directly
    jj.git_fetch_refspec(&remote_url, &refspec)?;

    // Import the new ref into jj
    jj.git_import()?;

    eprintln!(
        "{} Fetched PR #{} as bookmark '{}'",
        style("✓").green().bold(),
        args.pr_number,
        local_branch,
    );

    // Track the PR so it appears in `jc status`.
    match repo::get_single_commit(jj.repo_root(), &local_branch) {
        Ok(commit) => {
            pr_store.set(&commit.change_id, args.pr_number)?;
        }
        Err(e) => {
            tracing::warn!("Could not track PR: {}", e);
        }
    }

    if args.rebase {
        jj.rebase(&local_branch, "@")?;
        eprintln!(
            "{} Rebased '{}' onto current commit",
            style("✓").green().bold(),
            local_branch,
        );
    }

    if args.checkout {
        jj.new_commit(&local_branch)?;
        eprintln!(
            "{} Checked out bookmark '{}'",
            style("✓").green().bold(),
            local_branch,
        );
    }

    Ok(())
}
