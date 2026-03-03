use crate::config::Config;
use crate::gh::{Gh, GhAuth};
use crate::jj::{CommandRunner, DefaultRunner, Jj};
use crate::repo;
use anyhow::{anyhow, Context as _, Result};
use clap::Args;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Args, Debug)]
pub struct SubmitArgs {
    /// The revset to submit
    #[arg(short = 'r', long = "revset", default_value = "@")]
    pub revset: String,
}

#[derive(Deserialize, Debug)]
struct JjLogCommit {
    commit_id: String,
    change_id: String,
    description: String,
    parents: Vec<String>,
}

struct StackPr {
    commit_id: String,
    pr_number: Option<u32>,
    description: String,
}

pub struct SubmitContext<'a> {
    pub config: &'a Config,
    pub runner: Arc<dyn CommandRunner + Send + Sync>,
}

pub fn run(args: &SubmitArgs, config: &Config) -> Result<()> {
    let ctx = SubmitContext {
        config,
        runner: Arc::new(DefaultRunner),
    };
    run_with_context(args, &ctx)
}

pub fn run_with_context(args: &SubmitArgs, ctx: &SubmitContext) -> Result<()> {
    let gh = Gh::new(Arc::clone(&ctx.runner));
    let auth = gh.auth_status()?;
    println!("Authenticated as GitHub user: {}", auth.login);

    let repo_root = repo::find_root()
        .ok_or_else(|| anyhow!("Not a jujutsu repository (or any parent directories): .jj"))?;

    let jj = Jj::with_runner(repo_root, Arc::clone(&ctx.runner));

    let output_str = jj
        .log_reversed(&args.revset, "json(self) ++ \"\\n\"")
        .context("jj log failed")?;

    let bookmark_prefix = ctx.config
        .extra
        .get("jellycat.bookmark_prefix")
        .cloned()
        .unwrap_or_else(|| "jellycat/".to_string());

    let upstream_repo = ctx.config
        .upstream
        .as_ref()
        .ok_or_else(|| anyhow!("jellycat.upstream not configured. Run 'jellycat init'."))?;

    let origin_remote = ctx.config.origin.as_deref().unwrap_or("origin");

    for line in output_str.lines().filter(|l| !l.is_empty()) {
        let commit: JjLogCommit = serde_json::from_str(line)
            .with_context(|| format!("Error parsing jj log JSON output. Line: {}", line))?;

        submit_commit(&jj, &gh, &commit, ctx, &auth, &bookmark_prefix, upstream_repo, origin_remote)?;
    }

    Ok(())
}

fn extract_pr_number(description: &str) -> Option<u32> {
    description
        .lines()
        .find_map(|line| line.trim().strip_prefix("PR: #")?.parse().ok())
}

fn get_stack_data(jj: &Jj, commit_id: &str) -> Result<(Vec<StackPr>, Vec<JjLogCommit>)> {
    let stack_json = jj.get_stack(commit_id).context("Failed to get stack")?;
    let mut stack_prs = Vec::new();
    let mut stack_commits = Vec::new();
    for line in stack_json {
        let sc: JjLogCommit = serde_json::from_str(&line)?;
        let pr_number = extract_pr_number(&sc.description);
        stack_prs.push(StackPr {
            commit_id: sc.commit_id.clone(),
            pr_number,
            description: sc.description.clone(),
        });
        stack_commits.push(sc);
    }
    Ok((stack_prs, stack_commits))
}

fn generate_stack_graph(
    stack: &[StackPr],
    current_commit: &JjLogCommit,
    upstream_repo: &str,
    stack_commits: &[JjLogCommit],
) -> (String, String) {
    let current_idx = stack.iter().position(|s| s.commit_id == current_commit.commit_id);
    let mut prev_pr = None;
    let mut next_pr = None;
    let mut review_link_suffix = String::new();

    if let Some(idx) = current_idx {
        if idx > 0 { prev_pr = stack[idx - 1].pr_number; }
        if idx < stack.len() - 1 { next_pr = stack[idx + 1].pr_number; }

        let current_pr_num = stack[idx].pr_number;
        let mut first_idx = idx;
        if let Some(pr_num) = current_pr_num {
            while first_idx > 0 && stack[first_idx - 1].pr_number == Some(pr_num) {
                first_idx -= 1;
            }
        }

        if let Some(base) = stack_commits[first_idx].parents.first() {
            if first_idx == idx {
                review_link_suffix = format!("changes/{}", current_commit.commit_id);
            } else {
                review_link_suffix = format!("changes/{}..{}", base, current_commit.commit_id);
            }
        }
    }

    let mut nav = Vec::new();
    if let Some(p) = prev_pr {
        nav.push(format!("[« Previous PR](https://github.com/{}/pull/{})", upstream_repo, p));
    }
    if let Some(n) = next_pr {
        nav.push(format!("[Next PR »](https://github.com/{}/pull/{})", upstream_repo, n));
    }

    let mut md = String::new();
    if !nav.is_empty() {
        md.push_str(&nav.join(" | "));
        md.push_str("\n\n---\n\n");
    }

    md.push_str("<details>\n<summary><b>Stack</b></summary>\n\n");
    for s in stack {
        let is_current = s.commit_id == current_commit.commit_id;
        let bullet = if is_current { "->" } else { "*" };
        let title = s.description.lines().next().unwrap_or("No description");

        if is_current {
            md.push_str(&format!("{} **(This PR)**: {}\n", bullet, title));
        } else if let Some(n) = s.pr_number {
            md.push_str(&format!("{} [PR #{}](https://github.com/{}/pull/{}): {}\n", bullet, n, upstream_repo, n, title));
        }
    }
    md.push_str("\n</details>\n\n<!-- jellycat -->\n");

    (md, review_link_suffix)
}

fn submit_commit(
    jj: &Jj,
    gh: &Gh,
    commit: &JjLogCommit,
    ctx: &SubmitContext,
    auth: &GhAuth,
    bookmark_prefix: &str,
    upstream_repo: &str,
    origin_remote: &str,
) -> Result<()> {
    let (stack_prs, stack_commits) = get_stack_data(jj, &commit.commit_id)?;
    let (stack_graph, review_suffix) =
        generate_stack_graph(&stack_prs, commit, upstream_repo, &stack_commits);
    let pr_number = extract_pr_number(&commit.description);

    let (bookmark_name, is_new) = if let Some(pr_num) = pr_number {
        (gh.pr_view_head_ref(upstream_repo, pr_num)?, false)
    } else {
        (
            format!(
                "{}{}",
                bookmark_prefix,
                &commit.change_id[..12.min(commit.change_id.len())]
            ),
            true,
        )
    };

    println!("Setting bookmark '{}' for commit {}", bookmark_name, commit.commit_id);
    jj.bookmark_set(&bookmark_name, &commit.commit_id)?;

    println!("Pushing bookmark '{}' to remote '{}'", bookmark_name, origin_remote);
    jj.git_push(origin_remote, &bookmark_name)?;

    if is_new {
        create_pr(jj, gh, commit, ctx, auth, upstream_repo, &bookmark_name, &stack_graph)?;
    } else {
        let nav_bar = nav_bar_from_graph(&stack_graph);
        update_pr(
            gh,
            commit,
            upstream_repo,
            pr_number.unwrap(),
            &stack_graph,
            &review_suffix,
            &nav_bar,
        )?;
    }

    Ok(())
}

fn nav_bar_from_graph(graph: &str) -> Vec<String> {
    graph.lines().next()
        .filter(|l| l.contains("Previous PR") || l.contains("Next PR"))
        .map(|l| l.split(" | ").map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

fn create_pr(
    jj: &Jj,
    gh: &Gh,
    commit: &JjLogCommit,
    ctx: &SubmitContext,
    auth: &GhAuth,
    upstream: &str,
    bookmark: &str,
    body_prefix: &str,
) -> Result<()> {
    println!("Creating Pull Request on GitHub...");
    let title = commit.description.lines().next().unwrap_or("No description");
    let body = format!("{}\n{}", body_prefix, commit.description);
    let base = gh.default_branch(upstream)?;
    let head_owner = ctx.config.head_repo.as_deref()
        .and_then(|r| r.split('/').next())
        .unwrap_or(&auth.login);
    let head = format!("{}:{}", head_owner, bookmark);

    let (pr_num, url) = gh.create_pr(
        upstream,
        title,
        &body,
        &head,
        &base,
        ctx.config.head_repo.as_deref(),
    )?;

    let mut new_desc = commit.description.trim_end().to_string();
    if !new_desc.is_empty() { new_desc.push_str("\n\n"); }
    new_desc.push_str(&format!("PR: #{}", pr_num));

    jj.describe(&commit.commit_id, &new_desc)?;
    println!("Pull Request created: {}", url);
    Ok(())
}

fn update_pr(
    gh: &Gh,
    commit: &JjLogCommit,
    upstream: &str,
    pr_num: u32,
    stack_graph_base: &str,
    review_suffix: &str,
    nav_bar: &[String],
) -> Result<()> {
    println!("Updating Pull Request #{} on GitHub...", pr_num);

    let current_body = gh.pr_view_body(upstream, pr_num)?;
    let user_content = current_body.split_once("<!-- jellycat -->")
        .map(|(_, rest)| rest.trim())
        .unwrap_or_else(|| commit.description.trim());

    let mut final_nav = nav_bar.to_vec();
    let review_link = format!(
        "[Review Changes in This PR](https://github.com/{}/pull/{}/{})",
        upstream, pr_num, review_suffix
    );
    if !final_nav.is_empty() {
        final_nav.insert((final_nav.len() + 1) / 2, review_link);
    } else {
        final_nav.push(review_link);
    }

    let mut full_stack_md = String::new();
    full_stack_md.push_str(&final_nav.join(" | "));
    full_stack_md.push_str("\n\n");
    if let Some(details) = stack_graph_base.split_once("<details>") {
        full_stack_md.push_str("<details>");
        full_stack_md.push_str(details.1);
    }

    let body = format!("{}\n{}", full_stack_md, user_content);
    gh.pr_edit_body(upstream, pr_num, &body)?;
    Ok(())
}
