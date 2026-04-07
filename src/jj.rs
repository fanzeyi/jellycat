use crate::error::{CommandError, check_output, format_cmd};
use eyre::{Result, eyre};
use std::fmt::Debug;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::Arc;
use tracing::instrument;

pub trait CommandRunner {
    fn run_output(&self, cmd: &mut Command) -> Result<Output>;
    fn run_status(&self, cmd: &mut Command) -> Result<bool>;

    fn check_output(&self, cmd: &mut Command) -> Result<Output> {
        let output = self.run_output(cmd)?;
        check_output(format_cmd(cmd), &output)?;
        Ok(output)
    }

    fn check_status(&self, cmd: &mut Command) -> Result<()> {
        self.check_output(cmd).map(|_| ())
    }
}

pub struct DefaultRunner;

impl CommandRunner for DefaultRunner {
    fn run_output(&self, cmd: &mut Command) -> Result<Output> {
        let cmd_str = format_cmd(cmd);
        cmd.output().map_err(|e| {
            CommandError {
                command: cmd_str,
                exit_code: None,
                stderr: e.to_string(),
            }
            .into()
        })
    }

    fn run_status(&self, cmd: &mut Command) -> Result<bool> {
        let cmd_str = format_cmd(cmd);
        let status = cmd.status().map_err(|e| -> eyre::Report {
            CommandError {
                command: cmd_str,
                exit_code: None,
                stderr: e.to_string(),
            }
            .into()
        })?;
        Ok(status.success())
    }
}

pub struct Jj {
    repo_root: PathBuf,
    runner: Arc<dyn CommandRunner + Send + Sync>,
}

impl Debug for Jj {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Jj")
            .field("repo_root", &self.repo_root)
            .finish()
    }
}

impl Jj {
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            runner: Arc::new(DefaultRunner),
        }
    }

    pub fn with_runner(repo_root: PathBuf, runner: Arc<dyn CommandRunner + Send + Sync>) -> Self {
        Self { repo_root, runner }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new("jj");
        cmd.arg("-R")
            .arg(&self.repo_root)
            .arg("--ignore-working-copy");
        cmd
    }

    fn log_cmd(&self, cmd: &Command) {
        tracing::debug!("Running jj: {:?}", cmd);
    }

    #[instrument]
    pub fn config_list(&self) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("config").arg("list");
        self.log_cmd(&cmd);
        let output = self.runner.check_output(&mut cmd)?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Parse `jj config list` output into `(key, value)` pairs with surrounding
    /// double-quotes stripped. Entries without `=` are skipped.
    ///
    /// When `prefix` is `Some`, only keys starting with it are returned.
    pub fn config_list_parsed(&self, prefix: Option<&str>) -> Result<Vec<(String, String)>> {
        let stdout = self.config_list()?;
        let mut out = Vec::new();
        for line in stdout.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if let Some(p) = prefix
                && !key.starts_with(p)
            {
                continue;
            }
            let mut value = value.trim();
            if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                value = &value[1..value.len() - 1];
            }
            out.push((key.to_string(), value.to_string()));
        }
        Ok(out)
    }

    #[instrument]
    pub fn config_set(&self, key: &str, value: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("config")
            .arg("set")
            .arg("--repo")
            .arg(key)
            .arg(value);
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    #[instrument]
    pub fn config_unset(&self, key: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("config").arg("unset").arg("--repo").arg(key);
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    #[instrument]
    pub fn git_remote_list(&self) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("git").arg("remote").arg("list");
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        check_output(cmd_str, &output)?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    #[instrument]
    pub fn log(&self, revset: &str, template: &str) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("log")
            .arg("-r")
            .arg(revset)
            .arg("--no-graph")
            .arg("--template")
            .arg(template);
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        check_output(cmd_str, &output)?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    #[instrument]
    pub fn log_reversed(&self, revset: &str, template: &str) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("log")
            .arg("-r")
            .arg(revset)
            .arg("--no-graph")
            .arg("--template")
            .arg(template)
            .arg("--reversed");
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        check_output(cmd_str, &output)?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    #[instrument]
    pub fn bookmark_set(&self, name: &str, revision: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("bookmark")
            .arg("set")
            .arg(name)
            .arg("-r")
            .arg(revision);
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        check_output(cmd_str, &output)
    }

    #[instrument]
    pub fn describe(&self, change_id: &str, message: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("describe")
            .arg("-r")
            .arg(change_id)
            .arg("-m")
            .arg(message);
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        check_output(cmd_str, &output)
    }

    #[instrument]
    pub fn get_stack(&self, revision: &str) -> Result<Vec<String>> {
        let revset = format!("(::{} | {}::) & mutable()", revision, revision);
        let mut cmd = self.cmd();
        cmd.arg("log")
            .arg("-r")
            .arg(&revset)
            .arg("--no-graph")
            .arg("--template")
            .arg("json(self) ++ \"\\n\"")
            .arg("--reversed");
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        check_output(cmd_str, &output)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect())
    }

    #[instrument]
    pub fn bookmark_track(&self, name: &str, remote: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("bookmark")
            .arg("track")
            .arg(name)
            .arg("--remote")
            .arg(remote);
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        check_output(cmd_str, &output)
    }

    #[instrument]
    pub fn abandon(&self, change_ids: &[&str]) -> Result<bool> {
        if change_ids.is_empty() {
            return Ok(true);
        }
        let mut cmd = self.cmd();
        cmd.arg("abandon");
        for id in change_ids {
            cmd.arg("-r").arg(id);
        }
        self.log_cmd(&cmd);
        self.runner.run_status(&mut cmd)
    }

    #[instrument]
    pub fn git_push(&self, remote: &str, bookmark: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("git")
            .arg("push")
            .arg("--remote")
            .arg(remote)
            .arg("--bookmark")
            .arg(bookmark);
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    fn cmd_wc(&self) -> Command {
        let mut cmd = Command::new("jj");
        cmd.arg("-R").arg(&self.repo_root);
        cmd
    }

    /// Fetch a refspec from a remote URL via the `git` binary.
    /// Used by `jc get` because `jj git fetch` doesn't support arbitrary refspecs.
    #[instrument]
    pub fn git_fetch_refspec(&self, remote_url: &str, refspec: &str) -> Result<()> {
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(&self.repo_root)
            .arg("fetch")
            .arg("--no-tags")
            .arg(remote_url)
            .arg(refspec);
        tracing::debug!("Running git: {:?}", cmd);
        self.runner.check_status(&mut cmd)
    }

    #[instrument]
    pub fn git_import(&self) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("git").arg("import");
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    #[instrument]
    pub fn new_commit(&self, revision: &str) -> Result<()> {
        let mut cmd = self.cmd_wc();
        cmd.arg("new").arg(revision);
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    #[instrument]
    pub fn rebase(&self, branch: &str, destination: &str) -> Result<()> {
        let mut cmd = self.cmd_wc();
        cmd.arg("rebase")
            .arg("-b")
            .arg(branch)
            .arg("-d")
            .arg(destination);
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    #[instrument]
    pub fn find_upstream_remote(&self, upstream_repo: &str) -> Result<String> {
        let output = self.git_remote_list()?;
        for line in output.lines() {
            if let Some((name, url)) = line.split_once(' ')
                && url.contains(upstream_repo)
            {
                return Ok(name.to_string());
            }
        }
        Err(eyre!(
            "No remote found matching upstream repo '{}'",
            upstream_repo
        ))
    }

    /// List local bookmarks as `(name, change_id)` pairs.
    #[instrument]
    pub fn bookmark_list(&self, filter: Option<&str>) -> Result<Vec<(String, String)>> {
        let mut cmd = self.cmd();
        cmd.arg("bookmark")
            .arg("list")
            .arg("--template")
            .arg(r#"name ++ "\t" ++ self.normal_target().change_id() ++ "\n""#);
        if let Some(filter) = filter {
            cmd.arg(filter);
        }
        self.log_cmd(&cmd);
        let output = self.runner.check_output(&mut cmd)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();
        for line in stdout.lines() {
            if let Some((name, change_id)) = line.split_once('\t') {
                results.push((name.to_string(), change_id.to_string()));
            }
        }
        Ok(results)
    }

    #[instrument]
    pub fn bookmark_delete(&self, name: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("bookmark").arg("delete").arg(name);
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    pub fn repo_root(&self) -> &std::path::Path {
        &self.repo_root
    }

    #[instrument]
    pub fn git_push_bookmarks(&self, remote: &str, bookmarks: &[&str]) -> Result<()> {
        if bookmarks.is_empty() {
            return Ok(());
        }
        let mut cmd = self.cmd();
        cmd.arg("git").arg("push").arg("--remote").arg(remote);
        for b in bookmarks {
            cmd.arg("--bookmark").arg(b);
        }
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }
}

/// Extracts `owner/repo` from a GitHub remote URL.
/// Returns `None` for non-GitHub URLs.
pub fn parse_github_owner_repo(url: &str) -> Option<String> {
    let path = if let Some(rest) = url.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = url.strip_prefix("ssh://git@github.com/") {
        rest
    } else if let Some(rest) = url.strip_prefix("git@github.com:") {
        rest
    } else {
        return None;
    };

    let path = path.strip_suffix(".git").unwrap_or(path);
    let path = path.trim_end_matches('/');

    // Validate owner/repo format
    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_https() {
        assert_eq!(
            parse_github_owner_repo("https://github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            parse_github_owner_repo("https://github.com/owner/repo"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_parse_github_ssh() {
        assert_eq!(
            parse_github_owner_repo("git@github.com:owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            parse_github_owner_repo("git@github.com:owner/repo"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_parse_github_ssh_protocol() {
        assert_eq!(
            parse_github_owner_repo("ssh://git@github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            parse_github_owner_repo("ssh://git@github.com/owner/repo"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_parse_non_github() {
        assert_eq!(
            parse_github_owner_repo("https://gitlab.com/owner/repo.git"),
            None
        );
        assert_eq!(
            parse_github_owner_repo("git@bitbucket.org:owner/repo.git"),
            None
        );
    }

    #[test]
    fn test_parse_trailing_slash() {
        assert_eq!(
            parse_github_owner_repo("https://github.com/owner/repo/"),
            Some("owner/repo".to_string())
        );
    }
}
