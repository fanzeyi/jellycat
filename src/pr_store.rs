use crate::jj::Jj;
use eyre::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// Determines which backend stores PR ↔ change-id mappings.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum PrStoreType {
    #[default]
    Config,
    Bookmark,
}

pub trait PrStore: Send + Sync {
    fn list(&self) -> Result<HashMap<String, u32>>;
    fn set(&self, change_id: &str, pr_number: u32) -> Result<()>;
    fn unset(&self, change_id: &str) -> Result<()>;

    /// Find the change_id associated with a given PR number.
    /// Default implementation iterates `list()`.
    fn find_by_pr(&self, pr_number: u32) -> Result<Option<String>> {
        Ok(self
            .list()?
            .into_iter()
            .find(|(_, n)| *n == pr_number)
            .map(|(cid, _)| cid))
    }
}

// ---------------------------------------------------------------------------
// ConfigPrStore — stores mappings as `jellycat.prs.<change_id> = <num>`
// ---------------------------------------------------------------------------

pub struct ConfigPrStore {
    jj: Arc<Jj>,
}

impl PrStore for ConfigPrStore {
    fn list(&self) -> Result<HashMap<String, u32>> {
        let stdout = self.jj.config_list()?;
        let mut map = HashMap::new();
        for line in stdout.lines() {
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let mut value = value.trim();
                if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    value = &value[1..value.len() - 1];
                }
                if let Some(change_id) = key.strip_prefix("jellycat.prs.")
                    && let Ok(pr_num) = value.parse::<u32>()
                {
                    map.insert(change_id.to_string(), pr_num);
                }
            }
        }
        Ok(map)
    }

    fn set(&self, change_id: &str, pr_number: u32) -> Result<()> {
        let key = format!("jellycat.prs.{}", change_id);
        self.jj.config_set(&key, &pr_number.to_string())
    }

    fn unset(&self, change_id: &str) -> Result<()> {
        let key = format!("jellycat.prs.{}", change_id);
        self.jj.config_unset(&key)
    }
}

// ---------------------------------------------------------------------------
// BookmarkPrStore — stores mappings as local `pr-<NUM>` bookmarks
// ---------------------------------------------------------------------------

pub struct BookmarkPrStore {
    jj: Arc<Jj>,
}

impl PrStore for BookmarkPrStore {
    fn list(&self) -> Result<HashMap<String, u32>> {
        let bookmarks = self.jj.bookmark_list(Some("pr-*"))?;
        let mut map = HashMap::new();
        for (name, change_id) in bookmarks {
            if let Some(num_str) = name.strip_prefix("pr-")
                && let Ok(pr_num) = num_str.parse::<u32>()
            {
                map.insert(change_id, pr_num);
            }
        }
        Ok(map)
    }

    fn set(&self, change_id: &str, pr_number: u32) -> Result<()> {
        let name = format!("pr-{}", pr_number);
        self.jj.bookmark_set(&name, change_id)
    }

    fn unset(&self, change_id: &str) -> Result<()> {
        // Find the bookmark name for this change_id, then delete it.
        let bookmarks = self.jj.bookmark_list(Some("pr-*"))?;
        for (name, cid) in bookmarks {
            if cid == change_id && name.starts_with("pr-") {
                self.jj.bookmark_delete(&name)?;
                return Ok(());
            }
        }
        Ok(())
    }

    fn find_by_pr(&self, pr_number: u32) -> Result<Option<String>> {
        let name = format!("pr-{}", pr_number);
        let bookmarks = self.jj.bookmark_list(Some("pr-*"))?;
        Ok(bookmarks
            .into_iter()
            .find(|(n, _)| n == &name)
            .map(|(_, cid)| cid))
    }
}

/// Create a `PrStore` backend based on the configured type.
pub fn create(store_type: &PrStoreType, jj: Arc<Jj>) -> Box<dyn PrStore> {
    match store_type {
        PrStoreType::Config => Box::new(ConfigPrStore { jj }),
        PrStoreType::Bookmark => Box::new(BookmarkPrStore { jj }),
    }
}
