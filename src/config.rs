use crate::jj::Jj;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default)]
pub struct Config {
    pub upstream: Option<String>,
    pub origin: Option<String>,
    pub head_repo: Option<String>,
    pub github_user: Option<String>,
    pub prs: HashMap<String, u32>,
    pub extra: HashMap<String, String>,
}

pub fn load(repo_path: &Path) -> Result<Config> {
    let jj = Jj::new(repo_path.to_path_buf());
    let stdout = jj.config_list()?;
    let mut config = Config::default();

    for line in stdout.lines() {
        // Expected format: key = value
        // Note: jj config list output might depend on version, but typically key=value or key = value
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let mut value = value.trim();

            if key.starts_with("jellycat.") {
                // Strip quotes if present
                if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    value = &value[1..value.len() - 1];
                }

                if key == "jellycat.upstream" {
                    config.upstream = Some(value.to_string());
                } else if key == "jellycat.origin" {
                    config.origin = Some(value.to_string());
                } else if key == "jellycat.head_repo" {
                    config.head_repo = Some(value.to_string());
                } else if key == "jellycat.github_user" {
                    config.github_user = Some(value.to_string());
                } else if let Some(change_id) = key.strip_prefix("jellycat.prs.") {
                    if let Ok(pr_num) = value.parse::<u32>() {
                        config.prs.insert(change_id.to_string(), pr_num);
                    }
                } else {
                    config.extra.insert(key.to_string(), value.to_string());
                }
            }
        }
    }

    Ok(config)
}

pub fn save(repo_path: &Path, key: &str, value: &str) -> Result<()> {
    let jj = Jj::new(repo_path.to_path_buf());
    jj.config_set(key, value)
}
