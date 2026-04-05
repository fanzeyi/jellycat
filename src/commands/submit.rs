use crate::commands::CommandCtx;
use crate::config::Config;
use crate::jj::Jj;
use crate::pr_store::PrStore;
use clap::Args;
use console::style;
use eyre::{Context, Result, eyre};
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Args, Debug)]
pub struct SubmitArgs {
    /// The revset to submit
    #[arg(short = 'r', long = "revset")]
    pub revset: Option<String>,

    #[arg(short, long = "stack")]
    pub stack: bool,

    /// Create new PRs as draft
    #[arg(short, long = "draft")]
    pub draft: bool,
}

#[derive(Deserialize, Debug)]
struct JjLogCommit {
    commit_id: String,
    change_id: String,
    description: String,
    parents: Vec<String>,
}

struct PreparedCommit {
    commit: JjLogCommit,
    bookmark_name: String,
    is_new: bool,
}

struct StackCommit {
    commit_id: String,
    change_id: String,
    parents: Vec<String>,
    pr_number: Option<u32>,
}

struct StackGraph {
    /// Numbered list section — empty when there is only one PR in the stack.
    stack_md: String,
    nav_prev: Option<String>,
    nav_next: Option<String>,
    review_suffix: String,
}

fn plural(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("{} {}", n, singular)
    } else {
        format!("{} {}", n, plural)
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

fn new_spinner() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(running_spinner_style());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

fn format_bookmarks_for_debug(bookmarks: &[&str]) -> String {
    if bookmarks.is_empty() {
        "[]".to_string()
    } else if bookmarks.len() == 1 {
        format!("\"{}\"", bookmarks[0])
    } else if bookmarks.len() == 2 {
        format!("\"{}\", \"{}\"", bookmarks[0], bookmarks[1])
    } else {
        // only show the first 2, and indicate there are more
        format!(
            "\"{}\", \"{}\", and {} more",
            bookmarks[0],
            bookmarks[1],
            bookmarks.len() - 2
        )
    }
}

pub struct SubmitContext<'a> {
    pub cmd: CommandCtx,
    pub config: &'a Config,
    pub pr_store: &'a dyn PrStore,
}

pub fn run(args: &SubmitArgs, config: &Config, pr_store: &dyn PrStore) -> Result<()> {
    let cmd = CommandCtx::new()?;
    let ctx = SubmitContext {
        cmd,
        config,
        pr_store,
    };
    run_with_context(args, &ctx)
}

pub fn run_with_context(args: &SubmitArgs, ctx: &SubmitContext) -> Result<()> {
    let (gh, auth) = ctx.cmd.gh_with_auth(ctx.config)?;
    eprintln!(
        "{} Authenticated as {}",
        style("✓").green().bold(),
        style(&auth.login).bold()
    );

    let jj = Arc::clone(&ctx.cmd.jj);
    let gh = Arc::new(gh);

    let upstream_repo = ctx.cmd.require_upstream(ctx.config)?.to_string();
    let origin_remote = ctx
        .config
        .origin
        .as_ref()
        .ok_or_else(|| eyre!("jellycat.origin not configured. Run 'jc init'."))
        .cloned()?;

    // Parse the commits selected by the user's revset.
    let (commits, mut pr_map) = gather_commits(args, ctx, &jj)?;
    if commits.is_empty() {
        return Ok(());
    }
    let total = commits.len();

    // Phase 1: resolve/create bookmark names per commit.
    let prepared = phase_gather_bookmarks(
        Arc::clone(&gh),
        &upstream_repo,
        ctx.config.bookmark_prefix(),
        commits,
        &pr_map,
        total,
    )?;

    // Phase 2: set and track bookmarks locally.
    phase_setup_bookmarks(&jj, &prepared, &origin_remote)?;

    // Phase 3: push all bookmarks in one batch.
    phase_push_bookmarks(&jj, &prepared, &origin_remote, total)?;

    // Phase 4: create any new PRs in parallel.
    phase_create_prs(
        args,
        ctx,
        Arc::clone(&gh),
        &upstream_repo,
        &auth.login,
        &prepared,
        &mut pr_map,
    )?;

    // Merge all known PR mappings from config so the stack graph can reference
    // commits outside the current submit set (e.g. `jc submit -r b` in a-b-c).
    for (change_id, &pr_num) in &ctx.config.prs {
        pr_map.entry(change_id.clone()).or_insert(pr_num);
    }

    // Phase 5: rebuild each PR body with nav/stack info.
    phase_update_pr_bodies(
        Arc::clone(&jj),
        Arc::clone(&gh),
        &upstream_repo,
        &prepared,
        &pr_map,
        total,
    )?;

    // Persist new PR mappings.
    for p in prepared.iter().filter(|p| p.is_new) {
        let pr_num = pr_map[&p.commit.change_id];
        ctx.pr_store.set(&p.commit.change_id, pr_num)?;
    }

    print_summary(&prepared, &pr_map, &upstream_repo);

    Ok(())
}

/// Runs `jj log` over the target revset and returns the resolved commits plus
/// any PR numbers already known for them via config.
fn gather_commits(
    args: &SubmitArgs,
    ctx: &SubmitContext,
    jj: &Jj,
) -> Result<(Vec<JjLogCommit>, HashMap<String, u32>)> {
    let revset = if let Some(revset) = args.revset.as_deref() {
        revset
    } else if args.stack {
        "trunk()..@"
    } else {
        ctx.config.default_revset.as_deref().unwrap_or("@")
    };

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

    let pr_map: HashMap<String, u32> = commits
        .iter()
        .filter_map(|c| {
            ctx.config
                .prs
                .get(&c.change_id)
                .map(|&n| (c.change_id.clone(), n))
        })
        .collect();

    Ok((commits, pr_map))
}

/// Phase 1 — for each commit, either reuse an existing PR's head ref
/// (looked up from GitHub in parallel) or invent a fresh bookmark name.
fn phase_gather_bookmarks(
    gh: Arc<crate::gh::Gh>,
    upstream_repo: &str,
    bookmark_prefix: &str,
    commits: Vec<JjLogCommit>,
    pr_map: &HashMap<String, u32>,
    total: usize,
) -> Result<Vec<PreparedCommit>> {
    let pb = new_spinner();
    pb.set_message(format!(
        "Gathering PR info for {}...",
        plural(total, "commit", "commits")
    ));

    let handles: Vec<_> = commits
        .into_iter()
        .map(|commit| {
            let gh = Arc::clone(&gh);
            let upstream = upstream_repo.to_string();
            let prefix = bookmark_prefix.to_string();
            let pr_number = pr_map.get(&commit.change_id).copied();
            std::thread::spawn(move || -> Result<PreparedCommit> {
                let (bookmark_name, is_new) = if let Some(pr_num) = pr_number {
                    (gh.pr_view_head_ref(&upstream, pr_num)?, false)
                } else {
                    let random_suffix: String = rand::rng()
                        .sample_iter(rand::distr::Uniform::new_inclusive(b'a', b'z').unwrap())
                        .take(12)
                        .map(|b| b as char)
                        .collect();
                    let name = format!("{}{}", prefix, random_suffix);
                    (name, true)
                };
                Ok(PreparedCommit {
                    commit,
                    bookmark_name,
                    is_new,
                })
            })
        })
        .collect();

    let mut results = Vec::with_capacity(handles.len());
    let mut errors: Vec<eyre::Error> = Vec::new();
    for handle in handles {
        match handle.join().expect("thread panicked") {
            Ok(p) => results.push(p),
            Err(e) => errors.push(e),
        }
    }
    if let Some(e) = errors.into_iter().next() {
        pb.finish_and_clear();
        return Err(e);
    }
    pb.finish_and_clear();
    Ok(results)
}

/// Phase 2 — set each bookmark locally and `jj bookmark track` newly-created
/// ones so their first push gets a remote ref.
fn phase_setup_bookmarks(jj: &Jj, prepared: &[PreparedCommit], origin_remote: &str) -> Result<()> {
    let pb = new_spinner();
    pb.set_message("Setting up bookmarks...");
    for p in prepared {
        jj.bookmark_set(&p.bookmark_name, &p.commit.change_id)
            .context(format!("Failed to set bookmark {}", p.bookmark_name))?;
        if p.is_new {
            jj.bookmark_track(&p.bookmark_name, origin_remote)
                .context(format!(
                    "Failed to track bookmark {} in JJ",
                    p.bookmark_name
                ))?;
        }
    }
    pb.set_style(success_spinner_style());
    pb.finish_with_message("Bookmarks set up");
    Ok(())
}

/// Phase 3 — push every bookmark to the origin remote in a single network call.
fn phase_push_bookmarks(
    jj: &Jj,
    prepared: &[PreparedCommit],
    origin_remote: &str,
    total: usize,
) -> Result<()> {
    let pb = new_spinner();
    pb.set_message(format!(
        "Pushing {} to {}...",
        plural(total, "bookmark", "bookmarks"),
        origin_remote
    ));
    let all_names: Vec<&str> = prepared.iter().map(|p| p.bookmark_name.as_str()).collect();
    jj.git_push_bookmarks(origin_remote, &all_names)
        .with_context(|| {
            eyre!(
                "Failed to push bookmark {} to remote \"{}\"",
                format_bookmarks_for_debug(&all_names),
                origin_remote
            )
        })?;
    pb.set_style(success_spinner_style());
    pb.finish_with_message(format!("Pushed {}", plural(total, "bookmark", "bookmarks")));
    Ok(())
}

/// Phase 4 — for each `is_new` prepared commit, create a PR via the GitHub
/// API in parallel and add the resulting number to `pr_map`.
fn phase_create_prs(
    args: &SubmitArgs,
    ctx: &SubmitContext,
    gh: Arc<crate::gh::Gh>,
    upstream_repo: &str,
    auth_login: &str,
    prepared: &[PreparedCommit],
    pr_map: &mut HashMap<String, u32>,
) -> Result<()> {
    let new_count = prepared.iter().filter(|p| p.is_new).count();
    if new_count == 0 {
        return Ok(());
    }

    let pb = new_spinner();
    pb.set_message(format!(
        "Creating {}...",
        plural(new_count, "new PR", "new PRs")
    ));

    let default_branch = gh.default_branch(upstream_repo).with_context(|| {
        eyre!(
            "Failed to get default branch for upstream repo \"{}\"",
            upstream_repo
        )
    })?;
    let head_owner = ctx
        .config
        .origin_repo
        .as_deref()
        .and_then(|r| r.split('/').next())
        .unwrap_or(auth_login)
        .to_string();

    let handles: Vec<_> = prepared
        .iter()
        .filter(|p| p.is_new)
        .map(|p| {
            let gh = Arc::clone(&gh);
            let upstream = upstream_repo.to_string();
            let bookmark_name = p.bookmark_name.clone();
            let change_id = p.commit.change_id.clone();
            let description = p.commit.description.clone();
            let head = format!("{}:{}", head_owner, &bookmark_name);
            let base = default_branch.clone();
            let head_repo = ctx.config.origin_repo.clone();
            let draft = args.draft || ctx.config.draft;
            let title = description
                .lines()
                .next()
                .unwrap_or("No description")
                .to_string();
            std::thread::spawn(move || -> Result<(String, u32, String)> {
                let (pr_num, url) = gh
                    .create_pr(
                        &upstream,
                        &title,
                        &commit_body(&description),
                        &head,
                        &base,
                        head_repo.as_deref(),
                        draft,
                    )
                    .with_context(|| {
                        eyre!("Failed to create PR for bookmark \"{}\"", bookmark_name)
                    })?;
                Ok((change_id, pr_num, url))
            })
        })
        .collect();

    let mut errors: Vec<eyre::Error> = Vec::new();
    for handle in handles {
        match handle.join().expect("thread panicked") {
            Ok((change_id, pr_num, _)) => {
                pr_map.insert(change_id, pr_num);
            }
            Err(e) => errors.push(e),
        }
    }
    if let Some(e) = errors.into_iter().next() {
        pb.finish_and_clear();
        return Err(e);
    }
    pb.set_style(success_spinner_style());
    pb.finish_with_message(format!(
        "Created {}",
        plural(new_count, "new PR", "new PRs")
    ));
    Ok(())
}

/// Phase 5 — rebuild every PR body in parallel with stack navigation and a
/// review-diff link while preserving any user edits below the `<!-- jellycat -->`
/// marker.
fn phase_update_pr_bodies(
    jj: Arc<Jj>,
    gh: Arc<crate::gh::Gh>,
    upstream_repo: &str,
    prepared: &[PreparedCommit],
    pr_map: &HashMap<String, u32>,
    total: usize,
) -> Result<()> {
    let pb = new_spinner();
    pb.set_message(format!(
        "Updating {}...",
        plural(total, "PR body", "PR bodies")
    ));

    let handles: Vec<_> = prepared
        .iter()
        .map(|p| {
            let jj = Arc::clone(&jj);
            let gh = Arc::clone(&gh);
            let pr_map = pr_map.clone();
            let change_id = p.commit.change_id.clone();
            let description = p.commit.description.clone();
            let upstream = upstream_repo.to_string();
            let is_new = p.is_new;
            std::thread::spawn(move || -> Result<()> {
                let pr_num = *pr_map.get(&change_id).ok_or_else(|| {
                    eyre!(
                        "No PR number for commit {}",
                        &change_id[..12.min(change_id.len())]
                    )
                })?;
                let stack = get_stack(&jj, &change_id, &pr_map)?;
                let graph = generate_stack_graph(&stack, &change_id, &upstream);

                let user_content = if is_new {
                    commit_body(&description)
                } else {
                    let current_body = gh.pr_view_body(&upstream, pr_num)?;
                    current_body
                        .split_once("<!-- jellycat -->")
                        .map(|(_, rest)| rest.trim().to_string())
                        .unwrap_or_else(|| commit_body(&description))
                };

                let review_link = format!(
                    "[Review changes](https://github.com/{}/pull/{}/{})",
                    upstream, pr_num, graph.review_suffix
                );
                // Nav: always « prev · review · next » with review in the middle.
                let mut nav = Vec::new();
                if let Some(prev) = graph.nav_prev {
                    nav.push(prev);
                }
                nav.push(review_link);
                if let Some(next) = graph.nav_next {
                    nav.push(next);
                }
                let nav_line = nav.join(" · ");

                let prefix = if graph.stack_md.is_empty() {
                    nav_line
                } else {
                    format!("{}\n\n{}", nav_line, graph.stack_md)
                };
                let body = format!("{}\n\n<!-- jellycat -->\n\n{}", prefix, user_content);

                gh.pr_edit_body(&upstream, pr_num, &body)?;
                Ok(())
            })
        })
        .collect();

    let mut errors: Vec<_> = Vec::new();
    for handle in handles {
        if let Err(e) = handle.join().expect("thread panicked") {
            errors.push(e);
        }
    }
    if let Some(e) = errors.into_iter().next() {
        pb.finish_and_clear();
        return Err(e);
    }
    pb.set_style(success_spinner_style());
    pb.finish_with_message(format!("Updated {}", plural(total, "PR body", "PR bodies")));
    Ok(())
}

/// Pretty-print the per-PR summary emitted at the end of `jc submit`.
fn print_summary(prepared: &[PreparedCommit], pr_map: &HashMap<String, u32>, upstream_repo: &str) {
    eprintln!("\n");
    for (i, p) in prepared.iter().rev().enumerate() {
        let pr_num = pr_map[&p.commit.change_id];
        let title = p
            .commit
            .description
            .lines()
            .next()
            .unwrap_or("(no description)");
        let change_short = &p.commit.change_id[..12.min(p.commit.change_id.len())];
        let action = if p.is_new { "created" } else { "updated" };
        let pr_url = format!("https://github.com/{}/pull/{}", upstream_repo, pr_num);
        let pr_link = format!(
            "\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\",
            pr_url,
            style(format!("#{}", pr_num)).cyan().bold(),
        );
        let action_styled = if p.is_new {
            style(action).green().bold()
        } else {
            style(action).yellow()
        };
        let is_last = i == prepared.len() - 1;
        let bullet = style("◆").cyan().bold();
        let connector = if is_last { "┊" } else { "│" };
        eprintln!(
            "{} {} {} {} {}",
            bullet,
            style(change_short).magenta(),
            action_styled,
            pr_link,
            style(&pr_url).dim(),
        );
        eprintln!("{} {}", style(connector).cyan(), title);
    }
    eprintln!();
}

/// Returns the body of a commit description — everything after the first line (title).
fn commit_body(description: &str) -> String {
    let trimmed = description.trim();
    match trimmed.find('\n') {
        Some(pos) => trimmed[pos..].trim().to_string(),
        None => String::new(),
    }
}

fn get_stack(jj: &Jj, change_id: &str, pr_map: &HashMap<String, u32>) -> Result<Vec<StackCommit>> {
    jj.get_stack(change_id)
        .with_context(|| eyre!("Failed to get stack for change_id \"{}\"", change_id))?
        .iter()
        .map(|line| {
            let raw: JjLogCommit = serde_json::from_str(line)?;
            let pr_number = pr_map.get(&raw.change_id).copied();
            Ok(StackCommit {
                pr_number,
                commit_id: raw.commit_id,
                change_id: raw.change_id,
                parents: raw.parents,
            })
        })
        .collect()
}

fn generate_stack_graph(
    stack: &[StackCommit],
    current_change_id: &str,
    upstream_repo: &str,
) -> StackGraph {
    let Some(idx) = stack.iter().position(|s| s.change_id == current_change_id) else {
        return StackGraph {
            stack_md: String::new(),
            nav_prev: None,
            nav_next: None,
            review_suffix: String::new(),
        };
    };

    // Adjacent PR numbers for prev/next navigation.
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

    // Review-diff suffix: single commit → changes/<sha>, multi-commit PR → changes/<base>..<head>.
    let current_pr_num = stack[idx].pr_number;
    let mut first_idx = idx;
    if let Some(pr_num) = current_pr_num {
        while first_idx > 0 && stack[first_idx - 1].pr_number == Some(pr_num) {
            first_idx -= 1;
        }
    }
    let current_commit_id = &stack[idx].commit_id;
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

    // Numbered list — only shown when there is more than one PR in the stack.
    // Uses bare `#N` references so GitHub auto-expands them into rich PR links.
    let pr_commits: Vec<&StackCommit> = stack.iter().filter(|s| s.pr_number.is_some()).collect();
    let stack_md = if pr_commits.len() > 1 {
        let current_pos = pr_commits
            .iter()
            .position(|s| s.change_id == current_change_id)
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut items = Vec::new();
        for (i, commit) in pr_commits.iter().enumerate() {
            let pr_num = commit.pr_number.unwrap();
            if commit.change_id == current_change_id {
                items.push(format!("{}. **#{}** ← *this PR*", i + 1, pr_num));
            } else {
                items.push(format!("{}. #{}", i + 1, pr_num));
            }
        }
        format!(
            "<details>\n<summary><b>Stack ({} of {})</b></summary>\n\n{}\n\n</details>\n",
            current_pos,
            pr_commits.len(),
            items.join("\n"),
        )
    } else {
        String::new()
    };

    StackGraph {
        stack_md,
        nav_prev: prev_pr.map(|p| {
            format!(
                "[« PR #{}](https://github.com/{}/pull/{})",
                p, upstream_repo, p
            )
        }),
        nav_next: next_pr.map(|n| {
            format!(
                "[PR #{} »](https://github.com/{}/pull/{})",
                n, upstream_repo, n
            )
        }),
        review_suffix,
    }
}
