use anyhow::{Result, anyhow};
use std::path::PathBuf;
use std::process::Command;

pub struct Jj {
    repo_root: PathBuf,
}

impl Jj {
    pub fn new(repo_root: PathBuf) -> Self {
        Self { repo_root }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new("jj");
        cmd.arg("-R").arg(&self.repo_root);
        cmd
    }

    fn log_cmd(&self, cmd: &Command) {
        tracing::debug!("Running jj: {:?}", cmd);
    }

    pub fn config_list(&self) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("config").arg("list");
        self.log_cmd(&cmd);
        let output = cmd
            .output()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "jj config list failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

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
        let status = cmd
            .status()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !status.success() {
            return Err(anyhow!("jj config set failed"));
        }

        Ok(())
    }

    pub fn git_remote_list(&self) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("git").arg("remote").arg("list");
        self.log_cmd(&cmd);
        let output = cmd
            .output()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "jj git remote list failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

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
        let output = cmd
            .output()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "jj log failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

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
        let output = cmd
            .output()
            .map_err(|e| anyhow!("Failed to execute jj log: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "jj log failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

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
        let status = cmd
            .status()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !status.success() {
            return Err(anyhow!("jj bookmark set failed"));
        }

        Ok(())
    }

    pub fn describe(&self, revset: &str, message: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("describe")
            .arg("-r")
            .arg(revset)
            .arg("-m")
            .arg(message);
        self.log_cmd(&cmd);
        let status = cmd
            .status()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !status.success() {
            return Err(anyhow!("jj describe failed"));
        }

        Ok(())
    }

    pub fn get_stack(&self, revision: &str) -> Result<Vec<String>> {
        // Find all mutable ancestors and descendants.
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
        let output = cmd
            .output()
            .map_err(|e| anyhow!("Failed to execute jj log: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "jj log failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect())
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
        let status = cmd
            .status()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !status.success() {
            return Err(anyhow!("jj git push failed"));
        }

        Ok(())
    }
}
