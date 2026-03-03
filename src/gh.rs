use crate::jj::CommandRunner;
use anyhow::{anyhow, Context as _, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;

#[derive(Deserialize, Debug, Clone)]
pub struct GhAuth {
    pub login: String,
    pub token: String,
}

#[derive(Deserialize, Debug)]
struct GhAuthStatus {
    hosts: HashMap<String, Vec<GhAuth>>,
}

pub struct Gh {
    runner: Arc<dyn CommandRunner + Send + Sync>,
    token: Option<String>,
}

impl Gh {
    pub fn new(runner: Arc<dyn CommandRunner + Send + Sync>) -> Self {
        Self { runner, token: None }
    }

    pub fn with_token(runner: Arc<dyn CommandRunner + Send + Sync>, token: String) -> Self {
        Self { runner, token: Some(token) }
    }

    /// Retrieves a token for `user` using the existing gh authentication.
    /// Call this before constructing `Gh::with_token` for per-repo user support.
    pub fn get_token(runner: &Arc<dyn CommandRunner + Send + Sync>, user: &str) -> Result<String> {
        let mut cmd = Command::new("gh");
        cmd.args(["auth", "token", "--user", user]);
        tracing::debug!("Running gh: {:?}", cmd);
        let output = runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        if !output.status.success() {
            return Err(anyhow!(
                "gh auth token failed for user '{}': {}",
                user,
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new("gh");
        if let Some(token) = &self.token {
            cmd.env("GH_TOKEN", token);
        }
        cmd
    }

    fn log_cmd(&self, cmd: &Command) {
        tracing::debug!("Running gh: {:?}", cmd);
    }

    /// Extracts a human-readable error message from a failed `gh api` response.
    /// GitHub API errors return JSON in stdout: `{"message": "...", "errors": [...]}`.
    /// Falls back to stderr if the body can't be parsed.
    fn api_error(output: &std::process::Output) -> String {
        if let Ok(data) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            let mut parts = Vec::new();
            if let Some(msg) = data["message"].as_str() {
                parts.push(msg.to_string());
            }
            if let Some(errors) = data["errors"].as_array() {
                for e in errors {
                    if let Some(msg) = e["message"].as_str() {
                        parts.push(msg.to_string());
                    } else if let Some(s) = e.as_str() {
                        parts.push(s.to_string());
                    }
                }
            }
            if !parts.is_empty() {
                return parts.join(": ");
            }
        }
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    }

    pub fn auth_status(&self) -> Result<GhAuth> {
        let mut cmd = self.cmd();
        cmd.args(["auth", "status", "--json", "hosts", "--show-token"]);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)
            .context("Failed to execute 'gh auth status'")?;
        tracing::debug!("gh exited with status={}", output.status);
        if !output.status.success() {
            return Err(anyhow!(
                "gh auth status failed. Make sure you are logged in: 'gh auth login'"
            ));
        }
        let status: GhAuthStatus = serde_json::from_slice(&output.stdout)?;
        status
            .hosts
            .get("github.com")
            .and_then(|hosts| hosts.first())
            .cloned()
            .ok_or_else(|| anyhow!("No github.com authentication found. Run 'gh auth login'."))
    }

    pub fn pr_view_head_ref(&self, upstream: &str, pr_num: u32) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.args(["pr", "view", &pr_num.to_string(), "--repo", upstream, "--json", "headRefName"]);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        data["headRefName"]
            .as_str()
            .ok_or_else(|| anyhow!("No headRefName in response"))
            .map(|s| s.to_string())
    }

    pub fn pr_view_body(&self, upstream: &str, pr_num: u32) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.args(["pr", "view", &pr_num.to_string(), "--repo", upstream, "--json", "body"]);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        Ok(data["body"].as_str().unwrap_or("").to_string())
    }

    pub fn pr_edit_body(&self, upstream: &str, pr_num: u32, body: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.args(["pr", "edit", &pr_num.to_string(), "--repo", upstream, "--body", body]);
        self.log_cmd(&cmd);
        let ok = self.runner.run_status(&mut cmd)?;
        tracing::debug!("gh exited with success={}", ok);
        if !ok {
            return Err(anyhow!("gh pr edit failed"));
        }
        Ok(())
    }

    pub fn default_branch(&self, upstream: &str) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.args(["api", &format!("/repos/{}", upstream), "--jq", ".default_branch"]);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        if !output.status.success() {
            return Err(anyhow!("Failed to get default branch for {}: {}", upstream, Self::api_error(&output)));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn create_pr(
        &self,
        upstream: &str,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
        head_repo: Option<&str>,
    ) -> Result<(u32, String)> {
        let mut cmd = self.cmd();
        cmd.args([
            "api", "--method", "POST",
            &format!("/repos/{}/pulls", upstream),
            "-f", &format!("title={}", title),
            "-f", &format!("body={}", body),
            "-f", &format!("head={}", head),
            "-f", &format!("base={}", base),
        ]);
        if let Some(repo) = head_repo {
            cmd.args(["-f", &format!("head_repo={}", repo)]);
        }
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!(
            "gh exited with status={} stderr={:?}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );
        if !output.status.success() {
            return Err(anyhow!("Failed to create PR: {}", Self::api_error(&output)));
        }
        let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let pr_num = data["number"]
            .as_u64()
            .ok_or_else(|| anyhow!("No PR number in response"))? as u32;
        let url = data["html_url"]
            .as_str()
            .ok_or_else(|| anyhow!("No html_url in response"))?
            .to_string();
        Ok((pr_num, url))
    }
}
