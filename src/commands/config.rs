use crate::config::keys;
use crate::repo;
use eyre::{Result, eyre};

fn description(key: &str) -> Option<&'static str> {
    match key {
        keys::UPSTREAM_REPO => Some("GitHub owner/repo for upstream repository"),
        keys::UPSTREAM => Some("Git remote name for upstream"),
        keys::ORIGIN => Some("Git remote name for fork (origin)"),
        keys::ORIGIN_REPO => Some("GitHub owner/repo for fork repository"),
        keys::GITHUB_USER => Some("GitHub username for interacting with GitHub API"),
        keys::DRAFT => Some("Create PRs as drafts by default"),
        keys::BOOKMARK_PREFIX => Some("Prefix for jj bookmarks managed by jellycat"),
        keys::PR_STORE => Some("Backend for PR-to-change-id mappings (config or bookmark)"),
        keys::DEFAULT_REVSET => Some("Default revset used when submitting"),
        keys::PR_TEMPLATE => Some("Template for PR body"),
        _ => None,
    }
}

pub fn run() -> Result<()> {
    let repo_root = repo::find_root()
        .ok_or_else(|| eyre!("Not a jujutsu repository (or any of the parent directories): .jj"))?;

    let jj = crate::jj::Jj::new(repo_root);
    let entries = jj.config_list_parsed(Some("jellycat."))?;

    if entries.is_empty() {
        eprintln!("No jellycat configuration found. Run 'jc init' to set up.");
        return Ok(());
    }

    for (key, value) in &entries {
        if key.starts_with(keys::PRS_PREFIX) {
            continue;
        }
        if let Some(desc) = description(key) {
            println!("# {desc}");
        }
        println!("{key}={value}");
        println!();
    }

    Ok(())
}
