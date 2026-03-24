use crate::config::{self, Config};
use crate::jj::{DefaultRunner, Jj};
use crate::repo;
use anyhow::{Result, anyhow};
use clap::Args;
use console::style;
use std::process::Command;
use std::sync::Arc;

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

pub fn run(args: &GetArgs, config: &Config) -> Result<()> {
    let repo_root = repo::find_root()
        .ok_or_else(|| anyhow!("Not a jujutsu repository (or any parent directories): .jj"))?;

    let jj = Jj::with_runner(repo_root, Arc::new(DefaultRunner));

    let upstream_repo = config
        .upstream_repo
        .as_ref()
        .ok_or_else(|| anyhow!("jellycat.upstream_repo not configured. Run 'jc init'."))?;

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
        .ok_or_else(|| anyhow!("Could not find URL for remote '{}'", remote_name))?
        .to_string();

    // Run git fetch with the remote URL directly
    let repo_path = jj.repo_root();
    let status = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("fetch")
        .arg(&remote_url)
        .arg(&refspec)
        .status()
        .map_err(|e| anyhow!("Failed to run git fetch: {}", e))?;

    if !status.success() {
        return Err(anyhow!("git fetch failed"));
    }

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
            let key = format!("jellycat.prs.{}", commit.change_id);
            config::save(jj.repo_root(), &key, &args.pr_number.to_string())?;
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
