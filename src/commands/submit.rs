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
    change_id: String,
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

    let bookmark_prefix = config
        .extra
        .get("jellycat.bookmark_prefix")
        .cloned()
        .unwrap_or_else(|| "jellycat/".to_string());

    let upstream_repo = config
        .upstream
        .clone()
        .unwrap_or_else(|| {
            eprintln!("Error: jellycat.upstream not configured. Run 'jellycat init'.");
            exit(1);
        });

    let origin_remote = config
        .origin
        .clone()
        .unwrap_or_else(|| "origin".to_string());

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

        let mut pr_number = None;
        for desc_line in commit.description.lines() {
            if let Some(pr_num_str) = desc_line.trim().strip_prefix("PR: #") {
                if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                    pr_number = Some(pr_num);
                    break;
                }
            }
        }

        let bookmark_name = if let Some(pr_num) = pr_number {
            println!("Found PR #{} for commit {}, looking up bookmark name", pr_num, commit.commit_id);
            let pr_view_output = Command::new("gh")
                .arg("pr")
                .arg("view")
                .arg(pr_num.to_string())
                .arg("--repo")
                .arg(&upstream_repo)
                .arg("--json")
                .arg("headRefName")
                .output()
                .expect("Failed to execute gh pr view");

            if !pr_view_output.status.success() {
                eprintln!(
                    "gh pr view failed: {}",
                    String::from_utf8_lossy(&pr_view_output.stderr).trim()
                );
                exit(1);
            }

            let pr_data: serde_json::Value = serde_json::from_slice(&pr_view_output.stdout)
                .expect("Failed to parse gh pr view JSON");

            pr_data["headRefName"]
                .as_str()
                .expect("Could not find headRefName in PR view output")
                .to_string()
        } else {
            println!(
                "No PR found for commit {}, generating bookmark",
                commit.commit_id
            );
            format!("{}{}", bookmark_prefix, &commit.change_id[..12])
        };

        println!(
            "Setting bookmark '{}' for commit {}",
            bookmark_name, commit.commit_id
        );
        let status = Command::new("jj")
            .arg("bookmark")
            .arg("set")
            .arg(&bookmark_name)
            .arg("-r")
            .arg(&commit.commit_id)
            .arg("-R")
            .arg(&repo_root)
            .status()
            .expect("Failed to execute jj bookmark set");

        if !status.success() {
            eprintln!("Error: jj bookmark set failed");
            exit(1);
        }

        println!(
            "Pushing bookmark '{}' to remote '{}'",
            bookmark_name, origin_remote
        );
        let status = Command::new("jj")
            .arg("git")
            .arg("push")
            .arg("--remote")
            .arg(&origin_remote)
            .arg("--bookmark")
            .arg(&bookmark_name)
            .arg("-R")
            .arg(&repo_root)
            .status()
            .expect("Failed to execute jj git push");

        if !status.success() {
            eprintln!("Error: jj git push failed");
            exit(1);
        }
    }
}
