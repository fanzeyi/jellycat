use crate::repo;
use clap::Args;
use std::process::{Command, exit};

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

    // 1. Get the commit.
    let commit = match repo::get_single_commit(&repo_root, &args.revset) {
        Ok(commit) => commit,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };

    // 2. Check for existing PRs and filter them out if --force is used.
    let mut new_description_lines = Vec::new();
    for line in commit.description.lines() {
        if let Some(pr_num_str) = line.trim().strip_prefix("PR: #") {
            if !args.force {
                if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                    if pr_num == args.pr_number {
                        println!(
                            "PR #{} is already linked to changeset {}.",
                            args.pr_number, commit.change_id
                        );
                        return;
                    } else {
                        eprintln!(
                            "Commit is already linked to PR #{}. Use --force to overwrite.",
                            pr_num
                        );
                        exit(1);
                    }
                }
            }
        } else {
            new_description_lines.push(line);
        }
    }

    // 3. Construct the new description.
    let mut new_description = new_description_lines.join("\n").trim_end().to_string();
    if !new_description.is_empty() {
        new_description.push_str("\n\n");
    }
    new_description.push_str(&format!("PR: #{}", args.pr_number));

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

    println!(
        "Linked PR #{} to change {}",
        args.pr_number, commit.change_id
    );
}
