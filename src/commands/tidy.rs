use crate::commands::CommandCtx;
use crate::config::Config;
use crate::pr_store::PrStore;
use clap::Args;
use console::style;
use eyre::Result;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Args, Debug)]
pub struct TidyArgs {}

#[derive(Deserialize)]
struct JjLogCommit {
    change_id: String,
}

pub fn run(_args: &TidyArgs, config: &Config, pr_store: &dyn PrStore) -> Result<()> {
    let ctx = CommandCtx::new()?;
    let jj = &ctx.jj;

    if config.prs.is_empty() {
        eprintln!("No tracked PRs.");
        return Ok(());
    }

    // Find which tracked change IDs still exist in jj.
    let revset = config
        .prs
        .keys()
        .map(|id| id.as_str())
        .collect::<Vec<_>>()
        .join(" | ");

    let existing_change_ids: HashSet<String> = match jj.log(&revset, "json(self) ++ \"\\n\"") {
        Ok(output) => output
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(|l| serde_json::from_str::<JjLogCommit>(l).ok())
            .map(|c| c.change_id)
            .collect(),
        Err(_) => HashSet::new(),
    };

    // Collect abandoned change IDs (tracked but no longer in jj).
    let abandoned: Vec<(&String, &u32)> = config
        .prs
        .iter()
        .filter(|(change_id, _)| !existing_change_ids.contains(change_id.as_str()))
        .collect();

    // Query GitHub for PR states on remaining (non-abandoned) entries.
    let live_prs: Vec<(&String, &u32)> = config
        .prs
        .iter()
        .filter(|(change_id, _)| existing_change_ids.contains(change_id.as_str()))
        .collect();

    let to_tidy: Vec<(&String, &u32)> = if !live_prs.is_empty() {
        let gh = ctx.gh(config)?;
        let upstream = ctx.require_upstream(config)?;

        let pr_nums: Vec<u32> = live_prs.iter().map(|(_, pr)| **pr).collect();
        let states = gh.pr_states(upstream, &pr_nums)?;

        let closed: Vec<(&String, &u32)> = live_prs
            .into_iter()
            .filter(|(_, pr_num)| {
                states
                    .get(pr_num)
                    .map(|info| info.state == "CLOSED" || info.state == "MERGED")
                    .unwrap_or(false)
            })
            .collect();

        // Abandon changesets for closed/merged PRs.
        if !closed.is_empty() {
            let change_ids: Vec<&str> = closed.iter().map(|(cid, _)| cid.as_str()).collect();
            let _ = jj.abandon(&change_ids);
        }

        for (change_id, pr_num) in &closed {
            let state = states
                .get(pr_num)
                .map(|info| info.state.as_str())
                .unwrap_or("unknown");
            let change_short = &change_id[..12.min(change_id.len())];
            eprintln!(
                "{} [{}] PR #{} ({}) — removed",
                style("✓").green().bold(),
                change_short,
                pr_num,
                state.to_lowercase(),
            );
        }

        closed
    } else {
        vec![]
    };

    // Report abandoned entries.
    for (change_id, pr_num) in &abandoned {
        let change_short = &change_id[..12.min(change_id.len())];
        eprintln!(
            "{} [{}] PR #{} (abandoned) — removed",
            style("✓").green().bold(),
            change_short,
            pr_num,
        );
    }

    let total = to_tidy.len() + abandoned.len();

    if total == 0 {
        eprintln!("All tracked PRs are still open.");
        return Ok(());
    }

    // Remove PR mappings for both closed PRs and abandoned changesets.
    for (change_id, _) in to_tidy.iter().chain(abandoned.iter()) {
        pr_store.unset(change_id)?;
    }

    eprintln!("\nTidied {} PR mapping(s).", total);

    Ok(())
}
