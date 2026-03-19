use crate::config::Config;
use crate::gh::Gh;
use crate::jj::{DefaultRunner, Jj};
use crate::repo;
use anyhow::{Result, anyhow};
use clap::Args;
use console::style;
use std::sync::Arc;

#[derive(Args, Debug)]
pub struct TidyArgs {}

pub fn run(_args: &TidyArgs, config: &Config) -> Result<()> {
    let runner: Arc<dyn crate::jj::CommandRunner + Send + Sync> = Arc::new(DefaultRunner);

    let gh = if let Some(user) = &config.github_user {
        let token = Gh::get_token(&runner, user)?;
        Gh::with_token(Arc::clone(&runner), token)
    } else {
        let gh = Gh::new(Arc::clone(&runner));
        let _auth = gh.auth_status()?;
        gh
    };

    let repo_root = repo::find_root()
        .ok_or_else(|| anyhow!("Not a jujutsu repository (or any parent directories): .jj"))?;

    let jj = Jj::with_runner(repo_root, Arc::clone(&runner));

    let upstream = config
        .upstream
        .as_ref()
        .ok_or_else(|| anyhow!("jellycat.upstream not configured. Run 'jellycat init'."))?;

    if config.prs.is_empty() {
        eprintln!("No tracked PRs.");
        return Ok(());
    }

    let pr_nums: Vec<u32> = config.prs.values().copied().collect();
    let states = gh.pr_states(upstream, &pr_nums)?;

    let to_tidy: Vec<(&String, &u32)> = config
        .prs
        .iter()
        .filter(|(_, pr_num)| {
            states
                .get(pr_num)
                .map(|s| s == "CLOSED" || s == "MERGED")
                .unwrap_or(false)
        })
        .collect();

    if to_tidy.is_empty() {
        eprintln!("All tracked PRs are still open.");
        return Ok(());
    }

    let change_ids: Vec<&str> = to_tidy.iter().map(|(cid, _)| cid.as_str()).collect();
    let _ = jj.abandon(&change_ids);

    for (change_id, pr_num) in &to_tidy {
        let state = states.get(pr_num).map(|s| s.as_str()).unwrap_or("unknown");
        let key = format!("jellycat.prs.{}", change_id);
        jj.config_unset(&key)?;
        let change_short = &change_id[..12.min(change_id.len())];
        eprintln!(
            "{} [{}] PR #{} ({}) — removed",
            style("✓").green().bold(),
            change_short,
            pr_num,
            state.to_lowercase(),
        );
    }

    eprintln!(
        "\nTidied {} PR mapping(s).",
        to_tidy.len(),
    );

    Ok(())
}
