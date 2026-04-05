use crate::error::{check_output, format_cmd};
use crate::jj::CommandRunner;
use eyre::{Result, eyre};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct GhAuthAccount {
    pub login: String,
    pub host: String,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct PrInfo {
    pub state: String,
    pub comment_count: u32,
    pub failed_checks: u32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GhAuth {
    pub login: String,
    pub token: String,
}

#[derive(Deserialize, Debug)]
struct GhAuthStatus {
    hosts: HashMap<String, Vec<GhAuth>>,
}

// -- GraphQL response types for pr_states() --

#[derive(Deserialize)]
struct PrStatesResponse {
    data: PrStatesData,
}

#[derive(Deserialize)]
struct PrStatesData {
    repository: PrStatesRepository,
}

#[derive(Deserialize)]
struct PrStatesRepository {
    /// Dynamic PR aliases like "pr_42", "pr_99" etc.
    #[serde(flatten)]
    extra: HashMap<String, GqlPullRequest>,
}

#[derive(Deserialize)]
struct GqlPullRequest {
    state: String,
    comments: GqlCount,
    reviews: GqlCount,
    commits: GqlCommitConnection,
}

#[derive(Deserialize)]
struct GqlCount {
    #[serde(rename = "totalCount")]
    total_count: u32,
}

#[derive(Deserialize)]
struct GqlCommitConnection {
    nodes: Vec<GqlCommitNode>,
}

#[derive(Deserialize)]
struct GqlCommitNode {
    commit: GqlCommit,
}

#[derive(Deserialize)]
struct GqlCommit {
    #[serde(rename = "statusCheckRollup")]
    status_check_rollup: Option<GqlStatusCheckRollup>,
}

#[derive(Deserialize)]
struct GqlStatusCheckRollup {
    contexts: GqlCheckContextConnection,
}

#[derive(Deserialize)]
struct GqlCheckContextConnection {
    nodes: Vec<GqlCheckContext>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum GqlCheckContext {
    CheckRun { conclusion: Option<String> },
    StatusContext { state: String },
}

impl GqlCheckContext {
    fn is_failed(&self) -> bool {
        match self {
            GqlCheckContext::CheckRun { conclusion } => matches!(
                conclusion.as_deref(),
                Some("FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE")
            ),
            GqlCheckContext::StatusContext { state } => {
                matches!(state.as_str(), "FAILURE" | "ERROR")
            }
        }
    }
}

pub struct Gh {
    runner: Arc<dyn CommandRunner + Send + Sync>,
    token: Option<String>,
}

impl Gh {
    pub fn new(runner: Arc<dyn CommandRunner + Send + Sync>) -> Self {
        Self {
            runner,
            token: None,
        }
    }

    pub fn with_token(runner: Arc<dyn CommandRunner + Send + Sync>, token: String) -> Self {
        Self {
            runner,
            token: Some(token),
        }
    }

    /// Retrieves a token for `user` using the existing gh authentication.
    /// Call this before constructing `Gh::with_token` for per-repo user support.
    pub fn get_token(runner: &Arc<dyn CommandRunner + Send + Sync>, user: &str) -> Result<String> {
        let mut cmd = Command::new("gh");
        cmd.args(["auth", "token", "--user", user]);
        tracing::debug!("Running gh: {:?}", cmd);
        let cmd_str = format_cmd(&cmd);
        let output = runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        check_output(cmd_str, &output)?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn cmd(&self) -> Command {
        let binary = std::env::var("JELLYCAT_GH_BINARY").unwrap_or_else(|_| "gh".to_string());
        let mut cmd = Command::new(&binary);
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
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        check_output(cmd_str, &output)?;
        let status: GhAuthStatus = serde_json::from_slice(&output.stdout)?;
        status
            .hosts
            .get("github.com")
            .and_then(|hosts| hosts.first())
            .cloned()
            .ok_or_else(|| eyre!("No github.com authentication found. Run 'gh auth login'."))
    }

    pub fn list_accounts(&self) -> Result<Vec<GhAuthAccount>> {
        let mut cmd = self.cmd();
        cmd.args(["auth", "status", "--json", "hosts"]);
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        check_output(cmd_str, &output)?;

        #[derive(Deserialize)]
        struct Account {
            login: String,
            active: bool,
        }
        #[derive(Deserialize)]
        struct AuthStatus {
            hosts: HashMap<String, Vec<Account>>,
        }

        let status: AuthStatus = serde_json::from_slice(&output.stdout)?;
        let accounts: Vec<GhAuthAccount> = status
            .hosts
            .into_iter()
            .flat_map(|(host, accts)| {
                accts.into_iter().map(move |a| GhAuthAccount {
                    login: a.login,
                    host: host.clone(),
                    active: a.active,
                })
            })
            .collect();
        Ok(accounts)
    }

    /// Lists the authenticated user's open PRs on `upstream` as
    /// `(number, headRefName)` pairs.
    pub fn list_my_open_prs(&self, upstream: &str) -> Result<Vec<(u32, String)>> {
        let mut cmd = self.cmd();
        cmd.args([
            "pr",
            "list",
            "--repo",
            upstream,
            "--author",
            "@me",
            "--state",
            "open",
            "--json",
            "number,headRefName",
        ]);
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        check_output(cmd_str, &output)?;
        let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let arr = data
            .as_array()
            .ok_or_else(|| eyre!("Expected array from gh pr list"))?;
        let mut results = Vec::with_capacity(arr.len());
        for item in arr {
            let num = item["number"]
                .as_u64()
                .ok_or_else(|| eyre!("Missing number in gh pr list response"))?
                as u32;
            let head = item["headRefName"]
                .as_str()
                .ok_or_else(|| eyre!("Missing headRefName in gh pr list response"))?
                .to_string();
            results.push((num, head));
        }
        Ok(results)
    }

    pub fn pr_view_head_ref(&self, upstream: &str, pr_num: u32) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.args([
            "pr",
            "view",
            &pr_num.to_string(),
            "--repo",
            upstream,
            "--json",
            "headRefName",
        ]);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        data["headRefName"]
            .as_str()
            .ok_or_else(|| eyre!("No headRefName in response"))
            .map(|s| s.to_string())
    }

    pub fn pr_view_body(&self, upstream: &str, pr_num: u32) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.args([
            "pr",
            "view",
            &pr_num.to_string(),
            "--repo",
            upstream,
            "--json",
            "body",
        ]);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        Ok(data["body"].as_str().unwrap_or("").to_string())
    }

    pub fn pr_edit_body(&self, upstream: &str, pr_num: u32, body: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.args([
            "pr",
            "edit",
            &pr_num.to_string(),
            "--repo",
            upstream,
            "--body",
            body,
        ]);
        self.log_cmd(&cmd);
        let cmd_str = format_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        check_output(cmd_str, &output)
    }

    pub fn default_branch(&self, upstream: &str) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.args([
            "api",
            &format!("/repos/{}", upstream),
            "--jq",
            ".default_branch",
        ]);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);
        if !output.status.success() {
            return Err(eyre!(
                "Failed to get default branch for {}: {}",
                upstream,
                Self::api_error(&output)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Fetches the state and comment count of multiple PRs in a single GitHub GraphQL API call.
    pub fn pr_states(&self, upstream: &str, pr_nums: &[u32]) -> Result<HashMap<u32, PrInfo>> {
        if pr_nums.is_empty() {
            return Ok(HashMap::new());
        }

        let (owner, name) = upstream.split_once('/').ok_or_else(|| {
            eyre!(
                "Invalid upstream format '{}', expected 'owner/repo'",
                upstream
            )
        })?;

        // Per-PR fragment; `{n}` is replaced with the PR number for each alias.
        const PR_FRAGMENT: &str = r#"
  pr_{n}: pullRequest(number: {n}) {
    state
    comments { totalCount }
    reviews(first: 0) { totalCount }
    commits(last: 1) {
      nodes {
        commit {
          statusCheckRollup {
            contexts(first: 100) {
              nodes {
                __typename
                ... on CheckRun { conclusion }
                ... on StatusContext { state }
              }
            }
          }
        }
      }
    }
  }"#;

        let aliases: String = pr_nums
            .iter()
            .map(|n| PR_FRAGMENT.replace("{n}", &n.to_string()))
            .collect();

        let query = format!(
            r#"query {{
  repository(owner: "{owner}", name: "{name}") {{{aliases}
  }}
}}"#
        );

        let mut cmd = self.cmd();
        cmd.args(["api", "graphql", "-f", &format!("query={}", query)]);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!("gh exited with status={}", output.status);

        if !output.status.success() {
            return Err(eyre!("gh api graphql failed: {}", Self::api_error(&output)));
        }

        let response: PrStatesResponse = serde_json::from_slice(&output.stdout)?;

        let mut result = HashMap::new();
        for &pr_num in pr_nums {
            let alias = format!("pr_{}", pr_num);
            if let Some(pr_data) = response.data.repository.extra.get(&alias) {
                let failed_checks = pr_data
                    .commits
                    .nodes
                    .first()
                    .and_then(|n| n.commit.status_check_rollup.as_ref())
                    .map(|r| r.contexts.nodes.iter().filter(|c| c.is_failed()).count() as u32)
                    .unwrap_or(0);

                result.insert(
                    pr_num,
                    PrInfo {
                        state: pr_data.state.clone(),
                        comment_count: pr_data.comments.total_count + pr_data.reviews.total_count,
                        failed_checks,
                    },
                );
            }
        }

        Ok(result)
    }

    pub fn create_pr(
        &self,
        upstream: &str,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
        head_repo: Option<&str>,
        draft: bool,
    ) -> Result<(u32, String)> {
        let mut cmd = self.cmd();
        cmd.args([
            "api",
            "--method",
            "POST",
            &format!("/repos/{}/pulls", upstream),
            "-f",
            &format!("title={}", title),
            "-f",
            &format!("body={}", body),
            "-f",
            &format!("head={}", head),
            "-f",
            &format!("base={}", base),
        ]);
        if let Some(repo) = head_repo {
            cmd.args(["-f", &format!("head_repo={}", repo)]);
        }
        if draft {
            cmd.args(["-F", "draft=true"]);
        }
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        tracing::debug!(
            "gh exited with status={} stderr={:?}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );
        if !output.status.success() {
            return Err(eyre!("Failed to create PR: {}", Self::api_error(&output)));
        }
        let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let pr_num = data["number"]
            .as_u64()
            .ok_or_else(|| eyre!("No PR number in response"))? as u32;
        let url = data["html_url"]
            .as_str()
            .ok_or_else(|| eyre!("No html_url in response"))?
            .to_string();
        Ok((pr_num, url))
    }
}
