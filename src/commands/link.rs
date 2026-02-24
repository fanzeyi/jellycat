use crate::repo;
use clap::Args;
use std::process::{exit, Command};

#[derive(Args, Debug)]
pub struct LinkArgs {
    /// The revset to link a PR to (must resolve to a single commit)
    #[arg(short = 'r', long = "revset", default_value = "@")]
    pub revset: String,

    /// The PR number to link
    pub pr_number: u32,

    /// Overwrite existing PR link
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: &LinkArgs) {
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

    // 3. Check for existing PRs and filter them out if --force is used.
    for line in description.lines() {
        if let Some(pr_num_str) = line.trim().strip_prefix("PR: #") {
            if !args.force {
                if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                    if pr_num == args.pr_number {
                        println!("PR #{} is already linked to this commit.", args.pr_number);
                        return;
                    } else {
                        eprintln!("Commit is already linked to PR #{}. Use --force to overwrite.", pr_num);
                        exit(1);
                    }
                }
            }
        } else {
            new_description_lines.push(line);
        }
    }

    // 4. Construct the new description.
    let mut new_description = new_description_lines.join("\n").trim_end().to_string();
    if !new_description.is_empty() {
        new_description.push_str("\n\n");
    }
    new_description.push_str(&format!("PR: #{}", args.pr_number));

    // 5. Update commit description.
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

    println!("Linked PR #{} to commit {}", args.pr_number, commit_id);
}
