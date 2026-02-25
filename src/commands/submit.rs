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
    parents: Vec<String>,
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

    let repo_root = repo::find_root().ok_or_else(|| {
        anyhow::anyhow!("Not a jujutsu repository (or any of the parent directories): .jj")
    })?;

    let jj = Jj::new(repo_root);

    let output_str = jj
        .log_reversed(&args.revset, "json(self) ++ \"\\n\"")
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

    tracing::debug!("jj log output: {}", output_str);

    for line in output_str.lines() {
        if line.is_empty() {
            continue;
        }

        let commit: JjLogCommit = serde_json::from_str(line)
            .with_context(|| format!("Error parsing jj log JSON output. Line: {}", line))?;

        // 1. Get the stack for this commit.
        let stack_commits_json = jj
            .get_stack(&commit.commit_id)
            .context("Failed to get stack")?;
        let mut stack_commits: Vec<JjLogCommit> = Vec::new();
        for s_line in stack_commits_json {
            let s_commit: JjLogCommit = serde_json::from_str(&s_line)
                .with_context(|| format!("Error parsing stack commit JSON: {}", s_line))?;
            stack_commits.push(s_commit);
        }

        let mut prev_pr = None;
        let mut next_pr = None;
        let mut current_idx = None;

        let mut stack_prs = Vec::new();
        for sc in stack_commits.iter() {
            let mut pr_num = None;
            for desc_line in sc.description.lines() {
                if let Some(pr_num_str) = desc_line.trim().strip_prefix("PR: #") {
                    if let Ok(n) = pr_num_str.parse::<u32>() {
                        pr_num = Some(n);
                        break;
                    }
                }
            }
            stack_prs.push((sc.commit_id.clone(), pr_num, sc.description.clone()));
        }

        for (i, (cid, _, _)) in stack_prs.iter().enumerate() {
            if cid == &commit.commit_id {
                current_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = current_idx {
            if idx > 0 {
                prev_pr = stack_prs[idx - 1].1;
            }
            if idx < stack_prs.len() - 1 {
                next_pr = stack_prs[idx + 1].1;
            }
        }

        let mut pr_review_link = String::new();
        if let Some(idx) = current_idx {
            let current_pr_num = stack_prs[idx].1;
            let mut first_idx = idx;
            // Group contiguous commits with the same PR number
            if let Some(pr_num) = current_pr_num {
                while first_idx > 0 && stack_prs[first_idx - 1].1 == Some(pr_num) {
                    first_idx -= 1;
                }
            }

            let first_commit = &&stack_commits[first_idx];
            if let Some(base) = first_commit.parents.first() {
                if first_idx == idx {
                    // Single commit PR
                    pr_review_link = format!("changes/{}", commit.commit_id);
                } else {
                    // Multi-commit PR
                    pr_review_link = format!("changes/{}..{}", base, commit.commit_id);
                }
            }
        }

        let mut nav_bar = Vec::new();
        if let Some(p) = prev_pr {
            nav_bar.push(format!(
                "[« Previous PR](https://github.com/{}/pull/{})",
                upstream_repo, p
            ));
        }

        if let Some(n) = next_pr {
            nav_bar.push(format!(
                "[Next PR »](https://github.com/{}/pull/{})",
                upstream_repo, n
            ));
        }

        let mut stack_graph_md = String::new();
        if !nav_bar.is_empty() {
            stack_graph_md.push_str(&nav_bar.join(" | "));
            stack_graph_md.push_str("\n\n---\n\n");
        }
        stack_graph_md.push_str("<details>\n<summary><b>Stack</b></summary>\n\n");
        for (cid, pnum, desc) in stack_prs.iter() {
            let is_current = cid == &commit.commit_id;
            let bullet = if is_current { "->" } else { "*" };
            let title = desc.lines().next().unwrap_or("No description");

            if is_current {
                stack_graph_md.push_str(&format!("{} **(This PR)**: {}\n", bullet, title));
            } else if let Some(n) = pnum {
                stack_graph_md.push_str(&format!(
                    "{} [PR #{}](https://github.com/{}/pull/{}): {}\n",
                    bullet, n, upstream_repo, n, title
                ));
            }
        }
        stack_graph_md.push_str("\n</details>\n\n<!-- jellycat -->\n");

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
            // ... (existing bookmark lookup logic)
            println!(
                "Found PR #{} for commit {}, looking up bookmark name",
                pr_num, commit.commit_id
            );
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
            (
                format!("{}{}", bookmark_prefix, &commit.change_id[..12]),
                true,
            )
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
            let pr_body = format!("{}\n{}", stack_graph_md, commit.description);
            let pr_create_output = Command::new("gh")
                .arg("pr")
                .arg("create")
                .arg("--repo")
                .arg(&upstream_repo)
                .arg("--head")
                .arg(format!("{}:{}", username, bookmark_name))
                .arg("--title")
                .arg(
                    commit
                        .description
                        .lines()
                        .next()
                        .unwrap_or("No description"),
                )
                .arg("--body")
                .arg(&pr_body)
                .output()
                .context("Failed to execute gh pr create")?;

            if !pr_create_output.status.success() {
                return Err(anyhow::anyhow!(
                    "gh pr create failed: {}",
                    String::from_utf8_lossy(&pr_create_output.stderr).trim()
                ));
            }

            let pr_url = String::from_utf8_lossy(&pr_create_output.stdout)
                .trim()
                .to_string();
            println!("Pull Request created: {}", pr_url);

            // Extract PR number from URL (e.g., https://github.com/owner/repo/pull/123)
            let pr_num = pr_url
                .split('/')
                .last()
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or_else(|| anyhow::anyhow!("Failed to parse PR number from URL: {}", pr_url))?;

            println!("Linking PR #{} to commit {}", pr_num, commit.commit_id);
            let mut new_description = commit.description.trim_end().to_string();
            if !new_description.is_empty() {
                new_description.push_str("\n\n");
            }
            new_description.push_str(&format!("PR: #{}", pr_num));

            jj.describe(&commit.commit_id, &new_description)
                .context("jj describe failed to update commit with PR link")?;
        } else {
            let pr_num = pr_number.unwrap();
            println!("Updating Pull Request #{} on GitHub...", pr_num);

            // Fetch current PR body to preserve user edits
            let pr_view_body = Command::new("gh")
                .arg("pr")
                .arg("view")
                .arg(pr_num.to_string())
                .arg("--repo")
                .arg(&upstream_repo)
                .arg("--json")
                .arg("body")
                .output()
                .context("Failed to fetch existing PR body")?;

            let pr_body_data: serde_json::Value = serde_json::from_slice(&pr_view_body.stdout)
                .context("Failed to parse PR body JSON")?;

            let current_body = pr_body_data["body"].as_str().unwrap_or("");
            let user_content = if let Some((_, rest)) = current_body.split_once("<!-- jellycat -->")
            {
                rest.trim()
            } else {
                commit.description.trim()
            };

            // Add "Review on GitHub" button if we have the PR number
            let mut updated_nav = nav_bar.clone();
            let review_link = format!(
                "[Review Changes in This PR](https://github.com/{}/pull/{}/{})",
                upstream_repo, pr_num, pr_review_link
            );
            if !updated_nav.is_empty() {
                updated_nav.insert((updated_nav.len() + 1) / 2, review_link);
            } else {
                updated_nav.push(review_link);
            }

            let mut updated_stack_md = String::new();
            updated_stack_md.push_str(&updated_nav.join(" | "));
            updated_stack_md.push_str("\n\n<details>\n<summary><b>Stack</b></summary>\n\n");
            for (cid, pnum, _desc) in stack_prs.iter() {
                let is_current = cid == &commit.commit_id;
                let bullet = if is_current { "  *" } else { "*" };

                if let Some(n) = pnum {
                    if is_current {
                        updated_stack_md.push_str(&format!("{} #{} **⇤ Current**\n", bullet, n));
                    } else {
                        updated_stack_md.push_str(&format!("{} #{}\n", bullet, n));
                    }
                }
            }
            updated_stack_md.push_str("\n</details>\n\n----\n\n<!-- jellycat -->\n");

            let full_body = format!("{}\n{}", updated_stack_md, user_content);

            let pr_edit_status = Command::new("gh")
                .arg("pr")
                .arg("edit")
                .arg(pr_num.to_string())
                .arg("--repo")
                .arg(&upstream_repo)
                .arg("--body")
                .arg(&full_body)
                .status()
                .context("Failed to execute gh pr edit")?;

            if !pr_edit_status.success() {
                return Err(anyhow::anyhow!("gh pr edit failed"));
            }
        }
    }

    Ok(())
}
