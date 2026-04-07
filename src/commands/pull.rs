use crate::commands::CommandCtx;
use crate::config::Config;
use crate::pr_store::PrStore;
use clap::Args;
use console::style;
use eyre::{Context, Result};
use serde::Deserialize;

#[derive(Args, Debug)]
pub struct PullArgs {
    /// Revset of commits to pull PR descriptions into
    #[arg(short = 'r', long = "revset")]
    pub revset: Option<String>,
}

#[derive(Deserialize, Debug)]
struct JjLogCommit {
    change_id: String,
    description: String,
}

pub fn run(args: &PullArgs, config: &Config, pr_store: &dyn PrStore) -> Result<()> {
    let ctx = CommandCtx::new()?;
    run_with_ctx(args, config, pr_store, &ctx)
}

pub fn run_with_ctx(
    args: &PullArgs,
    config: &Config,
    pr_store: &dyn PrStore,
    ctx: &CommandCtx,
) -> Result<()> {
    let _ = pr_store; // used via config.prs
    let jj = &ctx.jj;
    let upstream = ctx.require_upstream(config)?;
    let gh = ctx.gh(config)?;

    let revset = args.revset.as_deref().unwrap_or("@");

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

    if commits.is_empty() {
        eprintln!("No commits matched revset '{}'", revset);
        return Ok(());
    }

    let mut pulled = 0;
    let mut skipped = 0;

    for commit in &commits {
        let pr_num = match config.prs.get(&commit.change_id) {
            Some(&n) => n,
            None => {
                eprintln!(
                    "{} {} — no PR associated, skipping",
                    style("~").yellow(),
                    &commit.change_id[..12],
                );
                skipped += 1;
                continue;
            }
        };

        let (title, body) = gh
            .pr_view_title_and_body(&upstream, pr_num)
            .with_context(|| format!("Failed to fetch PR #{}", pr_num))?;

        // Strip jellycat-generated section (everything before and including the marker)
        let user_body = body
            .split_once("<!-- jellycat -->")
            .map(|(_, rest)| rest.trim().to_string())
            .unwrap_or_else(|| body.trim().to_string());

        let new_description = if user_body.is_empty() {
            title.clone()
        } else {
            format!("{}\n\n{}", title, user_body)
        };

        if new_description == commit.description.trim() {
            eprintln!(
                "{} #{} {} — already up to date",
                style("✓").green(),
                pr_num,
                style(&title).dim(),
            );
            skipped += 1;
            continue;
        }

        jj.describe(&commit.change_id, &new_description)
            .with_context(|| format!("Failed to describe commit {}", &commit.change_id[..12]))?;

        eprintln!(
            "{} #{} {}",
            style("✓").green().bold(),
            pr_num,
            style(&title).dim(),
        );
        pulled += 1;
    }

    if pulled > 0 || skipped > 0 {
        eprintln!();
    }

    match pulled {
        0 => eprintln!("Nothing to pull."),
        1 => eprintln!("Pulled 1 commit description."),
        n => eprintln!("Pulled {} commit descriptions.", n),
    }

    Ok(())
}
