//! Shared command bootstrap.
//!
//! Every `jc` subcommand (except `init`) needs the same setup: find the `.jj`
//! root, build a `Jj` client, and authenticate against GitHub. `CommandCtx`
//! centralizes that boilerplate so each command's `run()` is free of duplicated
//! glue code.

use crate::config::Config;
use crate::gh::{Gh, GhAuth};
use crate::jj::{CommandRunner, DefaultRunner, Jj};
use crate::repo;
use eyre::{Result, eyre};
use std::path::PathBuf;
use std::sync::Arc;

pub struct CommandCtx {
    pub repo_root: PathBuf,
    pub jj: Arc<Jj>,
    pub runner: Arc<dyn CommandRunner + Send + Sync>,
}

impl CommandCtx {
    /// Build a context using the real subprocess runner.
    pub fn new() -> Result<Self> {
        Self::with_runner(Arc::new(DefaultRunner))
    }

    /// Build a context backed by a caller-supplied runner. Used by tests to
    /// inject a mock.
    pub fn with_runner(runner: Arc<dyn CommandRunner + Send + Sync>) -> Result<Self> {
        let repo_root = repo::find_root().ok_or_else(|| {
            eyre!("Not a jujutsu repository (or any of the parent directories): .jj")
        })?;
        let jj = Arc::new(Jj::with_runner(repo_root.clone(), Arc::clone(&runner)));
        Ok(Self {
            repo_root,
            jj,
            runner,
        })
    }

    /// Construct a `Gh` client according to the user's config. When
    /// `github_user` is set we pull a scoped token via `gh auth token`;
    /// otherwise we fall back to `gh auth status`.
    pub fn gh(&self, config: &Config) -> Result<Gh> {
        if let Some(user) = &config.github_user {
            let token = Gh::get_token(&self.runner, user)?;
            Ok(Gh::with_token(Arc::clone(&self.runner), token))
        } else {
            let gh = Gh::new(Arc::clone(&self.runner));
            let _ = gh.auth_status()?;
            Ok(gh)
        }
    }

    /// Like [`gh`](Self::gh) but also returns the authenticated `GhAuth`
    /// (login + token) — needed by `submit` which must know the login name
    /// when constructing PR head refs.
    pub fn gh_with_auth(&self, config: &Config) -> Result<(Gh, GhAuth)> {
        if let Some(user) = &config.github_user {
            let token = Gh::get_token(&self.runner, user)?;
            let gh = Gh::with_token(Arc::clone(&self.runner), token.clone());
            Ok((
                gh,
                GhAuth {
                    login: user.clone(),
                    token,
                },
            ))
        } else {
            let gh = Gh::new(Arc::clone(&self.runner));
            let auth = gh.auth_status()?;
            Ok((gh, auth))
        }
    }

    /// Returns the configured upstream repository or a user-facing error
    /// pointing at `jc init`.
    pub fn require_upstream<'a>(&self, config: &'a Config) -> Result<&'a str> {
        config
            .upstream_repo
            .as_deref()
            .ok_or_else(|| eyre!("jellycat.upstream_repo not configured. Run 'jc init'."))
    }
}
