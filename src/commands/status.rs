use crate::config::Config;
use crate::gh::Gh;
use crate::jj::{CommandRunner, DefaultRunner, Jj};
use crate::repo;
use anyhow::{Result, anyhow};
use clap::Args;
use console::style;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Args, Debug)]
pub struct StatusArgs {}

#[derive(Deserialize, Debug)]
struct JjLogCommit {
    commit_id: String,
    change_id: String,
    description: String,
    author: JjAuthor,
}

#[derive(Deserialize, Debug)]
struct JjAuthor {
    timestamp: String,
}

pub fn run(_args: &StatusArgs, config: &Config) -> Result<()> {
    let runner: Arc<dyn CommandRunner + Send + Sync> = Arc::new(DefaultRunner);

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
        .ok_or_else(|| anyhow!("jellycat.upstream not configured. Run 'jc init'."))?;

    if config.prs.is_empty() {
        eprintln!("No tracked PRs.");
        return Ok(());
    }

    // Fetch PR states in one GraphQL call.
    let pr_nums: Vec<u32> = config.prs.values().copied().collect();
    let states = gh.pr_states(upstream, &pr_nums)?;

    // Build a reverse map: change_id → pr_num for quick lookup.
    // Also collect all change_ids to query jj for commit info.
    let change_ids: Vec<&String> = config.prs.keys().collect();

    // Query jj for commit info on all tracked change IDs.
    let revset = change_ids
        .iter()
        .map(|id| id.as_str())
        .collect::<Vec<_>>()
        .join(" | ");

    let commits: HashMap<String, JjLogCommit> = match jj.log(&revset, "json(self) ++ \"\\n\"") {
        Ok(output) => output
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(|l| {
                serde_json::from_str::<JjLogCommit>(l)
                    .ok()
                    .map(|c| (c.change_id.clone(), c))
            })
            .collect(),
        Err(_) => HashMap::new(),
    };

    // Sort entries by PR number for stable output.
    let mut entries: Vec<(&String, &u32)> = config.prs.iter().collect();
    entries.sort_by_key(|(_, pr_num)| *pr_num);

    for (change_id, pr_num) in &entries {
        let info = states.get(pr_num);
        let state = info.map(|i| i.state.as_str()).unwrap_or("UNKNOWN");
        let comment_count = info.map(|i| i.comment_count).unwrap_or(0);

        let state_lower = state.to_lowercase();
        let state_styled = match state {
            "OPEN" => style(&state_lower).green(),
            "MERGED" => style(&state_lower).magenta(),
            "CLOSED" => style(&state_lower).red(),
            _ => style(&state_lower).dim(),
        };

        let pr_url = format!("https://github.com/{}/pull/{}", upstream, pr_num);
        let pr_link = format!(
            "\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\",
            pr_url,
            style(format!("#{}", pr_num)).bold(),
        );

        let comments_str = if comment_count > 0 {
            format!(" {}", style(format!("💬{}", comment_count)).dim())
        } else {
            String::new()
        };

        let change_short = &change_id[..12.min(change_id.len())];

        if let Some(commit) = commits.get(change_id.as_str()) {
            let title = commit
                .description
                .lines()
                .next()
                .unwrap_or("(no description)");
            let commit_short = &commit.commit_id[..8.min(commit.commit_id.len())];
            let timestamp = format_timestamp(&commit.author.timestamp);

            eprintln!(
                "{}  {} {} {} {} {}{}",
                style("○").bold(),
                style(change_short).magenta(),
                style(&timestamp).cyan(),
                style(commit_short).blue(),
                pr_link,
                state_styled,
                comments_str,
            );
            eprintln!("│  {}", title);
        } else {
            eprintln!(
                "{}  {} {} {}{}",
                style("○").bold(),
                style(change_short).magenta(),
                pr_link,
                state_styled,
                comments_str,
            );
            eprintln!("│  (commit not found)");
        }
    }

    Ok(())
}

/// Formats an RFC3339 timestamp into a human-friendly relative or short form.
/// Falls back to showing the raw string on parse failure.
fn format_timestamp(ts: &str) -> String {
    use std::time::SystemTime;

    // Parse RFC3339: "2026-03-18T22:08:45-07:00"
    // Extract the date portion and offset to compute a unix timestamp.
    let Some((date_time, offset_str)) = ts.rsplit_once(['+', '-']) else {
        return ts.to_string();
    };

    // Determine if the offset sign is + or -.
    // The split drops the delimiter, so check the char before offset_str in ts.
    let offset_sign_idx = ts.len() - offset_str.len() - 1;
    let offset_sign: i64 = if ts.as_bytes().get(offset_sign_idx) == Some(&b'-') {
        -1
    } else {
        1
    };

    // Parse offset "07:00" → seconds.
    let offset_parts: Vec<&str> = offset_str.split(':').collect();
    let offset_secs = if offset_parts.len() == 2 {
        let h: i64 = offset_parts[0].parse().unwrap_or(0);
        let m: i64 = offset_parts[1].parse().unwrap_or(0);
        offset_sign * (h * 3600 + m * 60)
    } else {
        0
    };

    // Parse "2026-03-18T22:08:45" naively.
    let parts: Vec<&str> = date_time.split('T').collect();
    if parts.len() != 2 {
        return ts.to_string();
    }
    let date_parts: Vec<u32> = parts[0].split('-').filter_map(|s| s.parse().ok()).collect();
    let time_parts: Vec<u32> = parts[1].split(':').filter_map(|s| s.parse().ok()).collect();
    if date_parts.len() != 3 || time_parts.len() < 2 {
        return ts.to_string();
    }

    let (year, month, day) = (
        date_parts[0] as i64,
        date_parts[1] as i64,
        date_parts[2] as i64,
    );
    let (hour, min, sec) = (
        time_parts[0] as i64,
        time_parts[1] as i64,
        time_parts.get(2).copied().unwrap_or(0) as i64,
    );

    // Days from epoch (simplified, no leap second handling).
    fn days_from_epoch(y: i64, m: i64, d: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y / 400;
        let yoe = y - era * 400;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    let epoch_secs =
        days_from_epoch(year, month, day) * 86400 + hour * 3600 + min * 60 + sec - offset_secs;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let diff = now - epoch_secs;

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let mins = diff / 60;
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if diff < 86400 {
        let hours = diff / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if diff < 86400 * 30 {
        let days = diff / 86400;
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", days)
        }
    } else {
        format!("{:04}-{:02}-{:02}", year, month, day)
    }
}
