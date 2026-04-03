use crate::jj::Jj;
use crate::pr_store::PrStoreType;
use eyre::Result;
use std::collections::HashMap;
use std::path::Path;

pub const DEFAULT_BOOKMARK_PREFIX: &str = "jellycat/";

#[derive(Debug, Default)]
pub struct Config {
    /// `owner/repo` string for the upstream repository
    pub upstream_repo: Option<String>,
    /// Git remote name for the upstream repository
    pub upstream: Option<String>,
    pub origin: Option<String>,
    /// `owner/repo` string for the fork (origin) repository
    pub origin_repo: Option<String>,
    pub github_user: Option<String>,
    pub bookmark_prefix: Option<String>,
    /// When true, new PRs are created as drafts by default.
    pub draft: bool,
    pub prs: HashMap<String, u32>,
    /// Which backend stores PR ↔ change-id mappings.
    pub pr_store_type: PrStoreType,
    /// Default revset to use when submitting
    pub default_revset: Option<String>,
    /// Old config keys that were found and should trigger deprecation warnings.
    /// Vec of (old_key, new_key) pairs.
    pub deprecated_keys: Vec<(&'static str, &'static str)>,
}

impl Config {
    pub fn bookmark_prefix(&self) -> &str {
        self.bookmark_prefix
            .as_deref()
            .unwrap_or(DEFAULT_BOOKMARK_PREFIX)
    }
}

pub fn load(repo_path: &Path) -> Result<Config> {
    let jj = Jj::new(repo_path.to_path_buf());
    let stdout = jj.config_list()?;
    let mut config = Config::default();

    // Track whether new-style keys are present
    let mut has_upstream_repo = false;
    let mut has_origin_repo = false;
    // Track old-style values for fallback
    let mut old_upstream_value: Option<String> = None;
    let mut old_head_repo_value: Option<String> = None;

    for line in stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let mut value = value.trim();

            if key.starts_with("jellycat.") {
                // Strip quotes if present
                if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    value = &value[1..value.len() - 1];
                }

                if key == "jellycat.upstream_repo" {
                    config.upstream_repo = Some(value.to_string());
                    has_upstream_repo = true;
                } else if key == "jellycat.upstream" {
                    // Could be old-style (owner/repo) or new-style (remote name).
                    // If it contains '/', treat as old-style owner/repo.
                    if value.contains('/') {
                        old_upstream_value = Some(value.to_string());
                    } else {
                        config.upstream = Some(value.to_string());
                    }
                } else if key == "jellycat.origin" {
                    config.origin = Some(value.to_string());
                } else if key == "jellycat.origin_repo" {
                    config.origin_repo = Some(value.to_string());
                    has_origin_repo = true;
                } else if key == "jellycat.head_repo" {
                    old_head_repo_value = Some(value.to_string());
                } else if key == "jellycat.github_user" {
                    config.github_user = Some(value.to_string());
                } else if key == "jellycat.draft" {
                    config.draft = value == "true";
                } else if key == "jellycat.bookmark_prefix" {
                    config.bookmark_prefix = Some(value.to_string());
                } else if key == "jellycat.pr_store" {
                    config.pr_store_type = match value {
                        "bookmark" => PrStoreType::Bookmark,
                        _ => PrStoreType::Config,
                    };
                } else if key == "jellycat.default_revset" {
                    config.default_revset = Some(value.to_string());
                }
            }
        }
    }

    // Fall back: old jellycat.upstream (owner/repo) → upstream_repo
    if !has_upstream_repo && let Some(val) = old_upstream_value {
        config.upstream_repo = Some(val);
        config
            .deprecated_keys
            .push(("jellycat.upstream", "jellycat.upstream_repo"));
    }

    // Fall back: old jellycat.head_repo → origin_repo
    if !has_origin_repo && let Some(val) = old_head_repo_value {
        config.origin_repo = Some(val);
        config
            .deprecated_keys
            .push(("jellycat.head_repo", "jellycat.origin_repo"));
    }

    Ok(config)
}

pub fn save(repo_path: &Path, key: &str, value: &str) -> Result<()> {
    let jj = Jj::new(repo_path.to_path_buf());
    jj.config_set(key, value)
}
