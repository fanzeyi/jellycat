use anyhow::{anyhow, Result};
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

    pub fn config_list(&self) -> Result<String> {
        let output = self.cmd()
            .arg("config")
            .arg("list")
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
        let status = self.cmd()
            .arg("config")
            .arg("set")
            .arg("--repo")
            .arg(key)
            .arg(value)
            .status()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !status.success() {
            return Err(anyhow!("jj config set failed"));
        }

        Ok(())
    }

    pub fn git_remote_list(&self) -> Result<String> {
        let output = self.cmd()
            .arg("git")
            .arg("remote")
            .arg("list")
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
        let output = self.cmd()
            .arg("log")
            .arg("-r")
            .arg(revset)
            .arg("--no-graph")
            .arg("--template")
            .arg(template)
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

    pub fn bookmark_set(&self, name: &str, revision: &str) -> Result<()> {
        let status = self.cmd()
            .arg("bookmark")
            .arg("set")
            .arg(name)
            .arg("-r")
            .arg(revision)
            .status()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !status.success() {
            return Err(anyhow!("jj bookmark set failed"));
        }

        Ok(())
    }

    pub fn describe(&self, revset: &str, message: &str) -> Result<()> {
        let status = self.cmd()
            .arg("describe")
            .arg("-r")
            .arg(revset)
            .arg("-m")
            .arg(message)
            .status()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !status.success() {
            return Err(anyhow!("jj describe failed"));
        }

        Ok(())
    }

    pub fn git_push(&self, remote: &str, bookmark: &str) -> Result<()> {
        let status = self.cmd()
            .arg("git")
            .arg("push")
            .arg("--remote")
            .arg(remote)
            .arg("--bookmark")
            .arg(bookmark)
            .status()
            .map_err(|e| anyhow!("Failed to execute jj: {}", e))?;

        if !status.success() {
            return Err(anyhow!("jj git push failed"));
        }

        Ok(())
    }
}
