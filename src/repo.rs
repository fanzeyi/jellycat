use serde::Deserialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Deserialize, Debug, Clone)]
pub struct JjLogCommit {
    pub change_id: String,
    pub description: String,
}

pub fn find_root() -> Option<PathBuf> {
    let mut current_dir = env::current_dir().ok()?;
    loop {
        if current_dir.join(".jj").is_dir() {
            return Some(current_dir);
        }
        if !current_dir.pop() {
            return None;
        }
    }
}

pub fn get_single_commit(repo_root: &Path, revset: &str) -> Result<JjLogCommit, String> {
    let output = Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg(revset)
        .arg("--no-graph")
        .arg("--template")
        .arg(r#"json(self)"#)
        .arg("-R")
        .arg(repo_root)
        .output()
        .map_err(|e| format!("Failed to execute jj log: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "jj log failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = output_str.lines().collect();
    if lines.len() != 1 {
        return Err(format!(
            "revset must resolve to exactly one commit, but got {}",
            lines.len()
        ));
    }

    serde_json::from_str(lines[0])
        .map_err(|e| format!("Error parsing jj log JSON output: {}. Line: {}", e, lines[0]))
}
