use crate::jj::Jj;
use serde::Deserialize;
use std::env;
use std::path::{Path, PathBuf};

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
    let jj = Jj::new(repo_root.to_path_buf());
    let output_str = jj.log(revset, "json(self)")?;
    
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
