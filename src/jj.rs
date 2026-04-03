use crate::error::{CommandError, check_output, format_cmd};
use eyre::{Result, eyre};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::Arc;

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

    pub fn config_list(&self) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("config").arg("list");
        self.log_cmd(&cmd);
        let output = self.runner.check_output(&mut cmd)?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

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

    pub fn config_unset(&self, key: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("config").arg("unset").arg("--repo").arg(key);
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    pub fn git_remote_list(&self) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("git").arg("remote").arg("list");
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        check_output(cmd_str, &output)?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

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

    pub fn git_import(&self) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("git").arg("import");
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    pub fn new_commit(&self, revision: &str) -> Result<()> {
        let mut cmd = self.cmd_wc();
        cmd.arg("new").arg(revision);
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

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
    pub fn bookmark_list(&self, filter: Option<&str>) -> Result<Vec<(String, String)>> {
        let mut cmd = self.cmd();
        cmd.arg("bookmark")
            .arg("list")
            .arg("--template")
            .arg(r#"name ++ "\t" ++ self.normal_target().commit_id().short(12) ++ "\n""#);
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

    pub fn bookmark_delete(&self, name: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("bookmark").arg("delete").arg(name);
        self.log_cmd(&cmd);
        self.runner.check_status(&mut cmd)
    }

    pub fn repo_root(&self) -> &std::path::Path {
        &self.repo_root
    }

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
