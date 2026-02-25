use crate::config::Config;
use crate::jj::Jj;
use crate::repo;
use anyhow::Context;
use clap::Args;
use serde::Deserialize;
use serde_json;
use std::process::Command;

#[derive(Args, Debug)]
pub struct SubmitArgs {
    /// The revset to submit
    #[arg(short = 'r', long = "revset", default_value = "@")]
    pub revset: String,
}

// Struct to deserialize the JSON output from jj log
#[derive(Deserialize, Debug)]
struct JjLogCommit {
    commit_id: String,
    change_id: String,
    description: String,
}

#[derive(Deserialize, Debug)]
struct GhHost {
    login: String,
    token: String,
}

#[derive(Deserialize, Debug)]
struct GhAuthStatus {
    hosts: std::collections::HashMap<String, Vec<GhHost>>,
}

pub fn run(args: &SubmitArgs, config: &Config) -> anyhow::Result<()> {
    let auth_output = Command::new("gh")
        .arg("auth")
        .arg("status")
        .arg("--json")
        .arg("hosts")
        .arg("--show-token")
        .output()?;

    if !auth_output.status.success() {
        return Err(anyhow::anyhow!(
            "gh auth status failed.\nMake sure you are logged in to GitHub CLI by running 'gh auth login'."
        ));
    }

    let auth_status: GhAuthStatus = serde_json::from_slice(&auth_output.stdout)
        .context("Error parsing 'gh auth status' JSON")?;

    // Try to find the token for github.com
    let (username, _github_token) = auth_status
        .hosts
        .get("github.com")
        .and_then(|hosts| hosts.first())
        .map(|h| (h.login.clone(), h.token.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No github.com authentication found. Please run 'gh auth login' to authenticate."
            )
        })?;

    println!("Authenticated as GitHub user: {}", username);

    let repo_root = repo::find_root()
        .ok_or_else(|| anyhow::anyhow!("Not a jujutsu repository (or any of the parent directories): .jj"))?;

    let jj = Jj::new(repo_root);

    let output_str = jj.log(&args.revset, "json(self)")
        .context("jj log failed")?;

    let bookmark_prefix = config
        .extra
        .get("jellycat.bookmark_prefix")
        .cloned()
        .unwrap_or_else(|| "jellycat/".to_string());

    let upstream_repo = config
        .upstream
        .clone()
        .ok_or_else(|| anyhow::anyhow!("jellycat.upstream not configured. Run 'jellycat init'."))?;

    let origin_remote = config
        .origin
        .clone()
        .unwrap_or_else(|| "origin".to_string());

    for line in output_str.lines() {
        if line.is_empty() {
            continue;
        }

        let commit: JjLogCommit = serde_json::from_str(line)
            .with_context(|| format!("Error parsing jj log JSON output. Line: {}", line))?;

        let mut pr_number = None;
        for desc_line in commit.description.lines() {
            if let Some(pr_num_str) = desc_line.trim().strip_prefix("PR: #") {
                if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                    pr_number = Some(pr_num);
                    break;
                }
            }
        }

        let (bookmark_name, is_new_pr) = if let Some(pr_num) = pr_number {
            println!("Found PR #{} for commit {}, looking up bookmark name", pr_num, commit.commit_id);
            let pr_view_output = Command::new("gh")
                .arg("pr")
                .arg("view")
                .arg(pr_num.to_string())
                .arg("--repo")
                .arg(&upstream_repo)
                .arg("--json")
                .arg("headRefName")
                .output()
                .context("Failed to execute gh pr view")?;

            if !pr_view_output.status.success() {
                return Err(anyhow::anyhow!(
                    "gh pr view failed: {}",
                    String::from_utf8_lossy(&pr_view_output.stderr).trim()
                ));
            }

            let pr_data: serde_json::Value = serde_json::from_slice(&pr_view_output.stdout)
                .context("Failed to parse gh pr view JSON")?;

            (
                pr_data["headRefName"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Could not find headRefName in PR view output"))?
                    .to_string(),
                false,
            )
        } else {
            println!(
                "No PR found for commit {}, generating bookmark",
                commit.commit_id
            );
            (format!("{}{}", bookmark_prefix, &commit.change_id[..12]), true)
        };

        println!(
            "Setting bookmark '{}' for commit {}",
            bookmark_name, commit.commit_id
        );
        jj.bookmark_set(&bookmark_name, &commit.commit_id)
            .context("jj bookmark set failed")?;

        println!(
            "Pushing bookmark '{}' to remote '{}'",
            bookmark_name, origin_remote
        );
        jj.git_push(&origin_remote, &bookmark_name)
            .context("jj git push failed")?;

        if is_new_pr {
            println!("Creating Pull Request on GitHub...");
            let pr_create_output = Command::new("gh")
                .arg("pr")
                .arg("create")
                .arg("--repo")
                .arg(&upstream_repo)
                .arg("--head")
                .arg(format!("{}:{}", username, bookmark_name))
                .arg("--title")
                .arg(commit.description.lines().next().unwrap_or("No description"))
                .arg("--body")
                .arg(&commit.description)
                .output()
                .context("Failed to execute gh pr create")?;

            if !pr_create_output.status.success() {
                return Err(anyhow::anyhow!(
                    "gh pr create failed: {}",
                    String::from_utf8_lossy(&pr_create_output.stderr).trim()
                ));
            }

            let pr_url = String::from_utf8_lossy(&pr_create_output.stdout).trim().to_string();
            println!("Pull Request created: {}", pr_url);

            // Extract PR number from URL (e.g., https://github.com/owner/repo/pull/123)
            let pr_number = pr_url
                .split('/')
                .last()
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or_else(|| anyhow::anyhow!("Failed to parse PR number from URL: {}", pr_url))?;

            println!("Linking PR #{} to commit {}", pr_number, commit.commit_id);
            let mut new_description = commit.description.trim_end().to_string();
            if !new_description.is_empty() {
                new_description.push_str("\n\n");
            }
            new_description.push_str(&format!("PR: #{}", pr_number));

            jj.describe(&commit.commit_id, &new_description)
                .context("jj describe failed to update commit with PR link")?;
        }
    }

    Ok(())
}
