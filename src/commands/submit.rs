use crate::config::Config;
use crate::gh::{Gh, GhAuth};
use crate::jj::{CommandRunner, DefaultRunner, Jj};
use crate::repo;
use anyhow::{Context as _, Result, anyhow};
use clap::Args;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

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

struct StackCommit {
    commit_id: String,
    parents: Vec<String>,
    description: String,
    pr_number: Option<u32>,
}

struct StackGraph {
    nav_items: Vec<String>,
    details_md: String,
    review_suffix: String,
}

impl StackGraph {
    fn to_body_prefix(&self) -> String {
        let mut md = String::new();
        if !self.nav_items.is_empty() {
            md.push_str(&self.nav_items.join(" | "));
            md.push_str("\n\n---\n\n");
        }
        md.push_str(&self.details_md);
        md
    }
}

/// Wraps a spinner and formats consistent step messages for a single commit submission.
struct Progress<'a> {
    pb: &'a ProgressBar,
    prefix: String,
}

impl<'a> Progress<'a> {
    fn new(pb: &'a ProgressBar, step: usize, total: usize, title: &str) -> Self {
        Self {
            pb,
            prefix: format!("[{}/{}] {}", step, total, style(title).bold()),
        }
    }

    fn set_action(&self, action: &str) {
        self.pb
            .set_message(format!("{} — {}", self.prefix, style(action).dim()));
    }

    fn finish_ok(&self, result: &str) {
        self.pb.set_style(success_spinner_style());
        self.pb
            .finish_with_message(format!("{} — {}", self.prefix, style(result).green()));
    }

    fn finish_err(&self, err: &str) {
        self.pb.set_style(error_spinner_style());
        self.pb
            .finish_with_message(format!("{} — {}", self.prefix, style(err).red()));
    }
}

fn running_spinner_style() -> ProgressStyle {
    ProgressStyle::default_spinner()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        .template("{spinner:.cyan} {msg}")
        .unwrap()
}

fn success_spinner_style() -> ProgressStyle {
    ProgressStyle::default_spinner()
        .tick_strings(&["✓"])
        .template("{spinner:.green} {msg}")
        .unwrap()
}

fn error_spinner_style() -> ProgressStyle {
    ProgressStyle::default_spinner()
        .tick_strings(&["✗"])
        .template("{spinner:.red} {msg}")
        .unwrap()
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
    let (gh, auth) = if let Some(user) = &ctx.config.github_user {
        let token = Gh::get_token(&ctx.runner, user)?;
        let gh = Gh::with_token(Arc::clone(&ctx.runner), token.clone());
        let auth = GhAuth {
            login: user.clone(),
            token,
        };
        (gh, auth)
    } else {
        let gh = Gh::new(Arc::clone(&ctx.runner));
        let auth = gh.auth_status()?;
        (gh, auth)
    };
    eprintln!(
        "{} Authenticated as {}",
        style("✓").green().bold(),
        style(&auth.login).bold()
    );

    let repo_root = repo::find_root()
        .ok_or_else(|| anyhow!("Not a jujutsu repository (or any parent directories): .jj"))?;

    let jj = Jj::with_runner(repo_root, Arc::clone(&ctx.runner));

    let output_str = jj
        .log_reversed(&args.revset, "json(self) ++ \"\\n\"")
        .context("jj log failed")?;

    let bookmark_prefix = ctx
        .config
        .extra
        .get("jellycat.bookmark_prefix")
        .cloned()
        .unwrap_or_else(|| "jellycat/".to_string());

    let upstream_repo = ctx
        .config
        .upstream
        .as_ref()
        .ok_or_else(|| anyhow!("jellycat.upstream not configured. Run 'jellycat init'."))?;

    let origin_remote = ctx.config.origin.as_deref().unwrap_or("origin");

    let commits: Vec<JjLogCommit> = output_str
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            serde_json::from_str(l)
                .with_context(|| format!("Error parsing jj log JSON output. Line: {}", l))
        })
        .collect::<Result<_>>()?;

    let total = commits.len();
    for (i, commit) in commits.iter().rev().enumerate() {
        let title = commit
            .description
            .lines()
            .next()
            .unwrap_or("(no description)");
        let pb = ProgressBar::new_spinner();
        pb.set_style(running_spinner_style());
        pb.enable_steady_tick(Duration::from_millis(80));
        let progress = Progress::new(&pb, i + 1, total, title);
        progress.set_action("starting...");

        if let Err(e) = submit_commit(
            &jj,
            &gh,
            commit,
            ctx,
            &auth,
            &bookmark_prefix,
            upstream_repo,
            origin_remote,
            &progress,
        ) {
            progress.finish_err(&e.to_string());
            return Err(e);
        }
    }

    Ok(())
}

fn extract_pr_number(description: &str) -> Option<u32> {
    description
        .lines()
        .find_map(|line| line.trim().strip_prefix("PR: #")?.parse().ok())
}

fn get_stack(jj: &Jj, commit_id: &str) -> Result<Vec<StackCommit>> {
    jj.get_stack(commit_id)
        .context("Failed to get stack")?
        .iter()
        .map(|line| {
            let raw: JjLogCommit = serde_json::from_str(line)?;
            Ok(StackCommit {
                pr_number: extract_pr_number(&raw.description),
                commit_id: raw.commit_id,
                parents: raw.parents,
                description: raw.description,
            })
        })
        .collect()
}

fn generate_stack_graph(
    stack: &[StackCommit],
    current_commit_id: &str,
    upstream_repo: &str,
) -> StackGraph {
    let current_idx = stack.iter().position(|s| s.commit_id == current_commit_id);

    let (nav_items, review_suffix) = if let Some(idx) = current_idx {
        let prev_pr = if idx > 0 {
            stack[idx - 1].pr_number
        } else {
            None
        };
        let next_pr = if idx < stack.len() - 1 {
            stack[idx + 1].pr_number
        } else {
            None
        };

        let current_pr_num = stack[idx].pr_number;
        let mut first_idx = idx;
        if let Some(pr_num) = current_pr_num {
            while first_idx > 0 && stack[first_idx - 1].pr_number == Some(pr_num) {
                first_idx -= 1;
            }
        }

        let review_suffix = stack[first_idx]
            .parents
            .first()
            .map(|base| {
                if first_idx == idx {
                    format!("changes/{}", current_commit_id)
                } else {
                    format!("changes/{}..{}", base, current_commit_id)
                }
            })
            .unwrap_or_default();

        let mut nav = Vec::new();
        if let Some(p) = prev_pr {
            nav.push(format!(
                "[« Previous PR](https://github.com/{}/pull/{})",
                upstream_repo, p
            ));
        }
        if let Some(n) = next_pr {
            nav.push(format!(
                "[Next PR »](https://github.com/{}/pull/{})",
                upstream_repo, n
            ));
        }

        (nav, review_suffix)
    } else {
        (Vec::new(), String::new())
    };

    let mut details_md = String::from("<details>\n<summary><b>Stack</b></summary>\n\n");
    for s in stack {
        let is_current = s.commit_id == current_commit_id;
        let title = s.description.lines().next().unwrap_or("No description");
        if is_current {
            details_md.push_str(&format!("-> **(This PR)**: {}\n", title));
        } else if let Some(n) = s.pr_number {
            details_md.push_str(&format!(
                "* [PR #{}](https://github.com/{}/pull/{}): {}\n",
                n, upstream_repo, n, title
            ));
        }
    }
    details_md.push_str("\n</details>\n\n<!-- jellycat -->\n");

    StackGraph {
        nav_items,
        details_md,
        review_suffix,
    }
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
    progress: &Progress,
) -> Result<()> {
    let stack = get_stack(jj, &commit.commit_id)?;
    let graph = generate_stack_graph(&stack, &commit.commit_id, upstream_repo);
    let pr_number = extract_pr_number(&commit.description);

    let (bookmark_name, is_new) = if let Some(pr_num) = pr_number {
        progress.set_action("fetching PR branch...");
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

    progress.set_action(&format!(
        "setting bookmark {}...",
        style(&bookmark_name).dim()
    ));
    jj.bookmark_set(&bookmark_name, &commit.commit_id)?;

    if is_new {
        progress.set_action(&format!(
            "tracking {} on {}...",
            style(&bookmark_name).dim(),
            origin_remote
        ));
        jj.bookmark_track(&bookmark_name, origin_remote)?;
    }

    progress.set_action(&format!(
        "pushing {} to {}...",
        style(&bookmark_name).dim(),
        origin_remote
    ));
    jj.git_push(origin_remote, &bookmark_name, &mut |line| {
        progress.pb.println(line)
    })?;

    if is_new {
        let url = create_pr(
            jj,
            gh,
            commit,
            ctx,
            auth,
            upstream_repo,
            &bookmark_name,
            origin_remote,
            &graph.to_body_prefix(),
            progress,
        )?;
        progress.finish_ok(&format!("PR created: {}", style(url).underlined()));
    } else {
        let pr_num = pr_number.unwrap();
        update_pr(gh, commit, upstream_repo, pr_num, &graph, progress)?;
        progress.finish_ok(&format!("PR #{} updated", pr_num));
    }

    Ok(())
}

fn create_pr(
    jj: &Jj,
    gh: &Gh,
    commit: &JjLogCommit,
    ctx: &SubmitContext,
    auth: &GhAuth,
    upstream: &str,
    bookmark: &str,
    origin_remote: &str,
    body_prefix: &str,
    progress: &Progress,
) -> Result<String> {
    let title = commit
        .description
        .lines()
        .next()
        .unwrap_or("No description");
    let body = format!("{}\n{}", body_prefix, commit.description);

    progress.set_action("fetching default branch...");
    let base = gh.default_branch(upstream)?;

    let head_owner = ctx
        .config
        .head_repo
        .as_deref()
        .and_then(|r| r.split('/').next())
        .unwrap_or(&auth.login);
    let head = format!("{}:{}", head_owner, bookmark);

    progress.set_action("creating PR...");
    let (pr_num, url) = gh.create_pr(
        upstream,
        title,
        &body,
        &head,
        &base,
        ctx.config.head_repo.as_deref(),
    )?;

    progress.set_action("updating commit description...");
    let mut new_desc = commit.description.trim_end().to_string();
    if !new_desc.is_empty() {
        new_desc.push_str("\n\n");
    }
    new_desc.push_str(&format!("PR: #{}", pr_num));
    jj.describe(&commit.commit_id, &new_desc)?;

    progress.set_action("pushing updated description...");
    jj.git_push(origin_remote, bookmark, &mut |line| progress.pb.println(line))?;

    Ok(url)
}

fn update_pr(
    gh: &Gh,
    commit: &JjLogCommit,
    upstream: &str,
    pr_num: u32,
    graph: &StackGraph,
    progress: &Progress,
) -> Result<()> {
    progress.set_action(&format!("fetching PR #{} body...", pr_num));
    let current_body = gh.pr_view_body(upstream, pr_num)?;
    let user_content = current_body
        .split_once("<!-- jellycat -->")
        .map(|(_, rest)| rest.trim())
        .unwrap_or_else(|| commit.description.trim());

    let review_link = format!(
        "[Review Changes in This PR](https://github.com/{}/pull/{}/{})",
        upstream, pr_num, graph.review_suffix
    );
    let mut final_nav = graph.nav_items.clone();
    if !final_nav.is_empty() {
        final_nav.insert((final_nav.len() + 1) / 2, review_link);
    } else {
        final_nav.push(review_link);
    }

    let body = format!(
        "{}\n\n{}\n{}",
        final_nav.join(" | "),
        graph.details_md,
        user_content
    );

    progress.set_action(&format!("updating PR #{}...", pr_num));
    gh.pr_edit_body(upstream, pr_num, &body)?;

    Ok(())
}
