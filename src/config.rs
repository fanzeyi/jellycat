use crate::jj::Jj;
use crate::pr_store::PrStoreType;
use eyre::Result;
use std::collections::HashMap;
use std::path::Path;

pub const DEFAULT_BOOKMARK_PREFIX: &str = "jellycat/";

/// Canonical config-key strings. All jellycat.* keys live here so renames and
/// deprecations are grep-able from a single location.
pub mod keys {
    pub const UPSTREAM_REPO: &str = "jellycat.upstream_repo";
    pub const UPSTREAM: &str = "jellycat.upstream";
    pub const ORIGIN: &str = "jellycat.origin";
    pub const ORIGIN_REPO: &str = "jellycat.origin_repo";
    pub const HEAD_REPO: &str = "jellycat.head_repo";
    pub const GITHUB_USER: &str = "jellycat.github_user";
    pub const DRAFT: &str = "jellycat.draft";
    pub const BOOKMARK_PREFIX: &str = "jellycat.bookmark_prefix";
    pub const PR_STORE: &str = "jellycat.pr_store";
    pub const DEFAULT_REVSET: &str = "jellycat.default_revset";
    pub const PRS_PREFIX: &str = "jellycat.prs.";
    pub const PR_TEMPLATE: &str = "jellycat.pr_template";
}

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
    /// Template for PR
    pub pr_template: Option<String>,
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
    let entries = jj.config_list_parsed(Some("jellycat."))?;
    load_from_entries(entries)
}

/// Build a `Config` from pre-parsed `(key, value)` entries. Exposed for tests.
pub fn load_from_entries(entries: Vec<(String, String)>) -> Result<Config> {
    let mut config = Config::default();

    // Track whether new-style keys are present
    let mut has_upstream_repo = false;
    let mut has_origin_repo = false;
    // Track old-style values for fallback
    let mut old_upstream_value: Option<String> = None;
    let mut old_head_repo_value: Option<String> = None;

    for (key, value) in entries {
        match key.as_str() {
            keys::UPSTREAM_REPO => {
                config.upstream_repo = Some(value);
                has_upstream_repo = true;
            }
            keys::UPSTREAM => {
                // Could be old-style (owner/repo) or new-style (remote name).
                // If it contains '/', treat as old-style owner/repo.
                if value.contains('/') {
                    old_upstream_value = Some(value);
                } else {
                    config.upstream = Some(value);
                }
            }
            keys::ORIGIN => {
                config.origin = Some(value);
            }
            keys::ORIGIN_REPO => {
                config.origin_repo = Some(value);
                has_origin_repo = true;
            }
            keys::HEAD_REPO => {
                old_head_repo_value = Some(value);
            }
            keys::GITHUB_USER => {
                config.github_user = Some(value);
            }
            keys::DRAFT => {
                config.draft = value == "true";
            }
            keys::BOOKMARK_PREFIX => {
                config.bookmark_prefix = Some(value);
            }
            keys::PR_STORE => {
                config.pr_store_type = match value.as_str() {
                    "bookmark" => PrStoreType::Bookmark,
                    _ => PrStoreType::Config,
                };
            }
            keys::DEFAULT_REVSET => {
                config.default_revset = Some(value);
            }
            keys::PR_TEMPLATE => {
                config.pr_template = Some(value);
            }
            _ => {}
        }
    }

    // Fall back: old jellycat.upstream (owner/repo) → upstream_repo
    if !has_upstream_repo && let Some(val) = old_upstream_value {
        config.upstream_repo = Some(val);
        config
            .deprecated_keys
            .push((keys::UPSTREAM, keys::UPSTREAM_REPO));
    }

    // Fall back: old jellycat.head_repo → origin_repo
    if !has_origin_repo && let Some(val) = old_head_repo_value {
        config.origin_repo = Some(val);
        config
            .deprecated_keys
            .push((keys::HEAD_REPO, keys::ORIGIN_REPO));
    }

    Ok(config)
}

pub fn save(repo_path: &Path, key: &str, value: &str) -> Result<()> {
    let jj = Jj::new(repo_path.to_path_buf());
    jj.config_set(key, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn loads_new_style_keys() {
        let cfg = load_from_entries(entries(&[
            ("jellycat.upstream_repo", "owner/upstream"),
            ("jellycat.upstream", "upstream"),
            ("jellycat.origin", "origin"),
            ("jellycat.origin_repo", "me/fork"),
            ("jellycat.github_user", "me"),
            ("jellycat.bookmark_prefix", "me/"),
            ("jellycat.draft", "true"),
            ("jellycat.default_revset", "trunk()..@"),
        ]))
        .unwrap();

        assert_eq!(cfg.upstream_repo.as_deref(), Some("owner/upstream"));
        assert_eq!(cfg.upstream.as_deref(), Some("upstream"));
        assert_eq!(cfg.origin.as_deref(), Some("origin"));
        assert_eq!(cfg.origin_repo.as_deref(), Some("me/fork"));
        assert_eq!(cfg.github_user.as_deref(), Some("me"));
        assert_eq!(cfg.bookmark_prefix.as_deref(), Some("me/"));
        assert!(cfg.draft);
        assert_eq!(cfg.default_revset.as_deref(), Some("trunk()..@"));
        assert!(cfg.deprecated_keys.is_empty());
    }

    #[test]
    fn migrates_deprecated_upstream_to_upstream_repo() {
        // Old-style: jellycat.upstream was the owner/repo string.
        let cfg = load_from_entries(entries(&[("jellycat.upstream", "owner/oldrepo")])).unwrap();

        assert_eq!(cfg.upstream_repo.as_deref(), Some("owner/oldrepo"));
        assert!(cfg.upstream.is_none());
        assert_eq!(
            cfg.deprecated_keys,
            vec![(keys::UPSTREAM, keys::UPSTREAM_REPO)]
        );
    }

    #[test]
    fn new_upstream_repo_wins_over_old_upstream_owner_slash_repo() {
        let cfg = load_from_entries(entries(&[
            ("jellycat.upstream_repo", "owner/new"),
            ("jellycat.upstream", "owner/old"),
        ]))
        .unwrap();

        assert_eq!(cfg.upstream_repo.as_deref(), Some("owner/new"));
        // No migration recorded because new key was present.
        assert!(cfg.deprecated_keys.is_empty());
    }

    #[test]
    fn migrates_head_repo_to_origin_repo() {
        let cfg = load_from_entries(entries(&[("jellycat.head_repo", "me/fork")])).unwrap();
        assert_eq!(cfg.origin_repo.as_deref(), Some("me/fork"));
        assert_eq!(
            cfg.deprecated_keys,
            vec![(keys::HEAD_REPO, keys::ORIGIN_REPO)]
        );
    }

    #[test]
    fn pr_store_type_parsing() {
        let cfg = load_from_entries(entries(&[("jellycat.pr_store", "bookmark")])).unwrap();
        assert_eq!(cfg.pr_store_type, PrStoreType::Bookmark);

        let cfg = load_from_entries(entries(&[("jellycat.pr_store", "config")])).unwrap();
        assert_eq!(cfg.pr_store_type, PrStoreType::Config);

        // Unknown value falls back to Config.
        let cfg = load_from_entries(entries(&[("jellycat.pr_store", "wat")])).unwrap();
        assert_eq!(cfg.pr_store_type, PrStoreType::Config);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let cfg = load_from_entries(entries(&[
            ("jellycat.something_new", "value"),
            ("jellycat.upstream_repo", "a/b"),
        ]))
        .unwrap();
        assert_eq!(cfg.upstream_repo.as_deref(), Some("a/b"));
    }
}
