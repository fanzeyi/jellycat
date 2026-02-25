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

#[derive(Deserialize, Debug)]
struct GhHost {
    login: String,
    token: String,
}

#[derive(Deserialize, Debug)]
struct GhAuthStatus {
    hosts: std::collections::HashMap<String, Vec<GhHost>>,
}

pub fn run(args: &SubmitArgs, config: &Config) {
    let auth_output = Command::new("gh")
        .arg("auth")
        .arg("status")
        .arg("--json")
        .arg("hosts")
        .arg("--show-token")
        .output();

    let auth_output = match auth_output {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Error: GitHub CLI ('gh') is not installed.");
            eprintln!("Please install it from https://cli.github.com/ or using your package manager.");
            exit(1);
        }
        Err(e) => {
            eprintln!("Error executing 'gh': {}", e);
            exit(1);
        }
    };

    if !auth_output.status.success() {
        eprintln!(
            "Error: gh auth status failed.\nMake sure you are logged in to GitHub CLI by running 'gh auth login'."
        );
        exit(1);
    }

    let auth_status: GhAuthStatus = match serde_json::from_slice(&auth_output.stdout) {
        Ok(status) => status,
        Err(e) => {
            eprintln!("Error parsing 'gh auth status' JSON: {}", e);
            exit(1);
        }
    };

    // Try to find the token for github.com
    let (username, _github_token) = auth_status
        .hosts
        .get("github.com")
        .and_then(|hosts| hosts.first())
        .map(|h| (h.login.clone(), h.token.clone()))
        .unwrap_or_else(|| {
            eprintln!("Error: No github.com authentication found.");
            eprintln!("Please run 'gh auth login' to authenticate.");
            exit(1);
        });

    println!("Authenticated as GitHub user: {}", username);

    println!("Submit command with revset: {}", args.revset);
    println!("Config: {:?}", config);
    // println!("GitHub Token: {}", github_token); // For debugging, usually don't print secrets

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
