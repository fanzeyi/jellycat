use crate::commands::CommandCtx;
use crate::commands::link::{LinkArgs, run_smart};
use crate::config::{self, keys};
use crate::gh::Gh;
use crate::jj::{self, DefaultRunner, Jj};
use crate::pr_store;
use crate::repo;
use clap::Args;
use console::style;
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};
use eyre::{Context, Result, eyre};
use std::sync::Arc;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Reconfigure upstream even if it's already configured
    #[arg(long)]
    force: bool,
}

struct RemoteInfo {
    name: String,
    #[allow(dead_code)]
    url: String,
    repo: Option<String>,
}

/// Find the index of a remote whose name matches a preferred name (e.g. "upstream", "origin").
fn find_default_index(remotes: &[RemoteInfo], preferred_name: &str) -> usize {
    remotes
        .iter()
        .position(|r| r.name == preferred_name)
        .unwrap_or(0)
}

fn select_remote_and_repo(
    remotes: &[RemoteInfo],
    prompt: &str,
    default_remote_name: &str,
) -> Result<(String, String)> {
    let theme = ColorfulTheme::default();

    let selected = if remotes.len() == 1 {
        let r = &remotes[0];
        let display = match &r.repo {
            Some(repo) => format!("{} — {}", style(&r.name).cyan(), style(repo).dim()),
            None => format!("{} — {}", style(&r.name).cyan(), style("(unknown)").dim()),
        };
        println!(
            "{} {}",
            style("Auto-selected remote:").green().bold(),
            display
        );
        0
    } else {
        let items: Vec<String> = remotes
            .iter()
            .map(|r| {
                let repo_display = r.repo.as_deref().unwrap_or("(unknown)");
                format!("{:<12} — {}", r.name, repo_display)
            })
            .collect();

        let default = find_default_index(remotes, default_remote_name);

        Select::with_theme(&theme)
            .with_prompt(prompt)
            .items(&items)
            .default(default)
            .interact()
            .context("Selection cancelled")?
    };

    let remote = &remotes[selected];

    let owner_repo: String = {
        let prompt = format!("GitHub repo for '{}' (owner/repo)", remote.name);
        let input = Input::<String>::with_theme(&theme)
            .with_prompt(prompt)
            .validate_with(|input: &String| validate_owner_repo(input));
        let input = match remote.repo {
            Some(ref default_repo) => input.with_initial_text(default_repo.clone()),
            None => input,
        };
        input.interact_text().context("Input cancelled")?
    };

    Ok((remote.name.clone(), owner_repo))
}

fn validate_owner_repo(input: &str) -> std::result::Result<(), String> {
    let parts: Vec<&str> = input.splitn(3, '/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Ok(())
    } else {
        Err("Expected 'owner/repo' format".to_string())
    }
}

/// Detect multiple GitHub accounts and prompt the user to select one.
/// Returns `Some(login)` if a selection was made, `None` otherwise.
fn select_github_account() -> Result<Option<String>> {
    let gh = Gh::new(Arc::new(DefaultRunner));
    let accounts = match gh.list_accounts() {
        Ok(a) => a,
        Err(_) => {
            println!(
                "{}",
                style("Warning: could not detect GitHub accounts. Run 'gh auth login' to authenticate.")
                    .yellow()
            );
            return Ok(None);
        }
    };

    if accounts.len() <= 1 {
        if accounts.is_empty() {
            println!(
                "{}",
                style("Warning: no GitHub accounts found. Run 'gh auth login' to authenticate.")
                    .yellow()
            );
        }
        return Ok(None);
    }

    let all_same_host = accounts.iter().all(|a| a.host == accounts[0].host);

    let items: Vec<String> = accounts
        .iter()
        .map(|a| {
            if all_same_host {
                a.login.clone()
            } else {
                format!("{} ({})", a.login, a.host)
            }
        })
        .collect();

    let default = accounts.iter().position(|a| a.active).unwrap_or(0);

    let theme = ColorfulTheme::default();
    let selected = Select::with_theme(&theme)
        .with_prompt("Select the GitHub account to use")
        .items(&items)
        .default(default)
        .interact()
        .context("Selection cancelled")?;

    Ok(Some(accounts[selected].login.clone()))
}

pub fn run(args: &InitArgs) -> Result<()> {
    let repo_root = repo::find_root()
        .ok_or_else(|| eyre!("Not a jujutsu repository (or any of the parent directories): .jj"))?;

    let jj = Jj::new(repo_root.clone());

    let config = config::load(&repo_root).context("Error loading config")?;

    if config.upstream_repo.is_some() && config.origin.is_some() && !args.force {
        println!(
            "{}",
            style(
                "jellycat upstream and origin are already configured. Use --force to reconfigure."
            )
            .yellow()
        );
        return Ok(());
    }

    // Fetch and parse remotes
    let remotes_str = jj.git_remote_list().context("Error listing git remotes")?;
    let remotes: Vec<RemoteInfo> = remotes_str
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_string();
            let url = parts.next()?.to_string();
            let repo = jj::parse_github_owner_repo(&url);
            Some(RemoteInfo { name, url, repo })
        })
        .collect();

    if remotes.is_empty() {
        return Err(eyre!(
            "No git remotes found. Please add a remote using 'jj git remote add'."
        ));
    }

    let (upstream_remote, upstream_repo) =
        select_remote_and_repo(&remotes, "Select the upstream remote", "upstream")?;

    let (origin_remote, origin_repo) =
        select_remote_and_repo(&remotes, "Select the origin remote", "origin")?;

    // GitHub account selection
    let selected_user = select_github_account()?;

    // Branch prefix selection
    let theme = ColorfulTheme::default();
    let default_prefix = match &selected_user {
        Some(user) => format!("jellycat/{}/", user),
        None => config::DEFAULT_BOOKMARK_PREFIX.to_string(),
    };
    let bookmark_prefix: String = Input::with_theme(&theme)
        .with_prompt("Branch prefix")
        .with_initial_text(&default_prefix)
        .interact_text()
        .context("Input cancelled")?;

    // Save all config keys
    config::save(&repo_root, keys::UPSTREAM, &upstream_remote)
        .context("Error saving upstream remote")?;
    config::save(&repo_root, keys::UPSTREAM_REPO, &upstream_repo)
        .context("Error saving upstream repo")?;
    config::save(&repo_root, keys::ORIGIN, &origin_remote).context("Error saving origin remote")?;
    config::save(&repo_root, keys::ORIGIN_REPO, &origin_repo)
        .context("Error saving origin repo")?;
    if let Some(ref user) = selected_user {
        config::save(&repo_root, keys::GITHUB_USER, user).context("Error saving github_user")?;
    }
    config::save(&repo_root, keys::BOOKMARK_PREFIX, &bookmark_prefix)
        .context("Error saving bookmark_prefix")?;

    println!();
    println!(
        "{}",
        style("jellycat configured successfully!").green().bold()
    );
    println!(
        "  {} {}",
        style("upstream remote:     ").bold(),
        style(&upstream_remote).cyan(),
    );
    println!(
        "  {} {}",
        style("upstream GitHub repo:").bold(),
        style(&upstream_repo).cyan()
    );
    println!(
        "  {} {}",
        style("origin remote:       ").bold(),
        style(&origin_remote).cyan(),
    );
    println!(
        "  {} {}",
        style("origin GitHub repo:  ").bold(),
        style(&origin_repo).cyan()
    );
    if let Some(ref user) = selected_user {
        println!(
            "  {} {}",
            style("GitHub user:         ").bold(),
            style(user).cyan()
        );
    }
    println!(
        "  {} {}",
        style("branch prefix:       ").bold(),
        style(&bookmark_prefix).cyan()
    );
    println!("");

    // Offer to auto-link any open PRs to matching local bookmarks.
    let theme = ColorfulTheme::default();
    if Confirm::with_theme(&theme)
        .with_prompt("Link existing open PRs?")
        .default(true)
        .interact()
        .context("Confirmation cancelled")?
    {
        let mut config = config::load(&repo_root).context("Error reloading config")?;
        let ctx = CommandCtx::new()?;
        let pr_store = pr_store::create(&config.pr_store_type, Arc::clone(&ctx.jj));
        config.prs = pr_store.list()?;
        let gh = ctx.gh(&config)?;
        let args = LinkArgs {
            revset: "@".to_string(),
            pr_number: None,
            force: false,
            smart: true,
        };
        run_smart(&args, &config, pr_store.as_ref(), &ctx, &gh, &upstream_repo)?;
    }

    Ok(())
}
