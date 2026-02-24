use crate::config::Config;
use crate::repo;
use clap::Args;
use serde_json;
use serde::Deserialize; // Re-add this import
use std::process::{exit, Command};

#[derive(Args, Debug)]
pub struct SubmitArgs {
    /// The revset to submit
    #[arg(short = 'r', long = "revset", default_value = "@")]
    pub revset: String,
}

// Struct to deserialize the JSON output from jj log
#[derive(Deserialize, Debug)]
struct JjLogCommit {
    commit_id: String,
    description: String,
}

pub fn run(args: &SubmitArgs, config: &Config) {
    println!("Submit command with revset: {}", args.revset);
    println!("Config: {:?}", config);

    let repo_root = match repo::find_root() {
        Some(root) => root,
        None => {
            eprintln!("Error: Not a jujutsu repository (or any of the parent directories): .jj");
            exit(1);
        }
    };

    let output = Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg(&args.revset)
        .arg("--no-graph")
        .arg("--template")
        .arg(r#"json(self)"#) // Changed template to json(self)
        .arg("-R")
        .arg(&repo_root)
        .output()
        .expect("Failed to execute jj log");

    if !output.status.success() {
        eprintln!(
            "jj log failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        exit(1);
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    for line in output_str.lines() {
        if line.is_empty() {
            continue;
        }

        let commit: JjLogCommit = match serde_json::from_str(line) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error parsing jj log JSON output: {}. Line: {}", e, line);
                exit(1);
            }
        };

        for desc_line in commit.description.lines() {
            if let Some(pr_num_str) = desc_line.trim().strip_prefix("PR: #") {
                if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                    println!("Found PR #{} for commit {}", pr_num, commit.commit_id);
                }
            }
        }
    }
}
