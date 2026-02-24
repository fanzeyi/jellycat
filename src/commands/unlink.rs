use crate::repo;
use clap::Args;
use std::process::{Command, exit};

#[derive(Args, Debug)]
pub struct UnlinkArgs {
    /// The revset to unlink a PR from (must resolve to a single commit)
    #[arg(short = 'r', long = "revset", default_value = "@")]
    pub revset: String,

    /// The PR number to unlink (optional, if not provided, all PRs are unlinked)
    pub pr_number: Option<u32>,
}

pub fn run(args: &UnlinkArgs) {
    let repo_root = match repo::find_root() {
        Some(root) => root,
        None => {
            eprintln!("Error: Not a jujutsu repository (or any of the parent directories): .jj");
            exit(1);
        }
    };

    // 1. Get the commit.
    let commit = match repo::get_single_commit(&repo_root, &args.revset) {
        Ok(commit) => commit,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };

    // 2. Filter out PR lines.
    let mut new_description_lines = Vec::new();
    let mut pr_unlinked = false;
    for line in commit.description.lines() {
        let trimmed_line = line.trim();
        let mut keep_line = true;
        if trimmed_line.starts_with("PR: #") {
            if let Some(pr_number_to_unlink) = args.pr_number {
                if let Some(pr_num_str) = trimmed_line.strip_prefix("PR: #") {
                    if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                        if pr_num == pr_number_to_unlink {
                            keep_line = false;
                            pr_unlinked = true;
                        }
                    }
                }
            } else {
                // No PR number specified, remove all PR lines.
                keep_line = false;
                pr_unlinked = true;
            }
        }
        if keep_line {
            new_description_lines.push(line);
        }
    }

    if !pr_unlinked {
        if let Some(pr_number) = args.pr_number {
            eprintln!(
                "Warning: PR #{} not found in commit description.",
                pr_number
            );
        } else {
            eprintln!("Warning: No PRs found in commit description to unlink.");
        }
        return;
    }

    let new_description = new_description_lines.join("\n");

    // 3. Update commit description.
    let status = Command::new("jj")
        .arg("describe")
        .arg("-r")
        .arg(&args.revset)
        .arg("-m")
        .arg(&new_description)
        .arg("-R")
        .arg(&repo_root)
        .status()
        .expect("Failed to execute jj describe");

    if !status.success() {
        eprintln!("jj describe failed");
        exit(1);
    }

    if let Some(pr_number) = args.pr_number {
        println!(
            "Unlinked PR #{} from changeset {}",
            pr_number, commit.change_id
        );
    } else {
        println!("Unlinked all PRs from changeset {}", commit.change_id);
    }
}
