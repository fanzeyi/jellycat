use crate::repo;
use clap::Args;
use std::process::{exit, Command};

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

    // 1. Verify the revset resolves to a single commit.
    let output = Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg(&args.revset)
        .arg("--no-graph")
        .arg("--template")
        .arg("commit_id")
        .arg("-R")
        .arg(&repo_root)
        .output()
        .expect("Failed to execute jj log");

    let commit_ids = String::from_utf8_lossy(&output.stdout);
    let commits: Vec<&str> = commit_ids.lines().collect();
    if commits.len() != 1 {
        eprintln!(
            "Error: revset must resolve to exactly one commit, but got {}",
            commits.len()
        );
        exit(1);
    }
    let commit_id = commits[0];

    // 2. Read existing description.
    let output = Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg(&args.revset)
        .arg("--no-graph")
        .arg("--template")
        .arg("description")
        .arg("-R")
        .arg(&repo_root)
        .output()
        .expect("Failed to execute jj log");

    let description = String::from_utf8_lossy(&output.stdout);
    let mut new_description_lines = Vec::new();
    let mut pr_unlinked = false;

    // 3. Filter out PR lines.
    for line in description.lines() {
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
            eprintln!("Warning: PR #{} not found in commit description.", pr_number);
        } else {
            eprintln!("Warning: No PRs found in commit description to unlink.");
        }
        return;
    }
    
    let new_description = new_description_lines.join("
");

    // 4. Update commit description.
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
        println!("Unlinked PR #{} from commit {}", pr_number, commit_id);
    } else {
        println!("Unlinked all PRs from commit {}", commit_id);
    }
}
