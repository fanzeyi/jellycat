use anyhow::{Result, anyhow};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

pub trait CommandRunner {
    fn run_output(&self, cmd: &mut Command) -> Result<Output>;
    fn run_status(&self, cmd: &mut Command) -> Result<bool>;
    /// Runs a command, calling `on_stderr` for each line written to stderr.
    /// Defaults to `run_status` (ignoring lines), used by mocks in tests.
    fn run_streaming(&self, cmd: &mut Command, _on_stderr: &mut dyn FnMut(&str)) -> Result<bool> {
        self.run_status(cmd)
    }
}

pub struct DefaultRunner;

impl CommandRunner for DefaultRunner {
    fn run_output(&self, cmd: &mut Command) -> Result<Output> {
        cmd.output()
            .map_err(|e| anyhow!("Failed to execute command: {}", e))
    }

    fn run_status(&self, cmd: &mut Command) -> Result<bool> {
        let status = cmd
            .status()
            .map_err(|e| anyhow!("Failed to execute command: {}", e))?;
        Ok(status.success())
    }

    fn run_streaming(&self, cmd: &mut Command, on_stderr: &mut dyn FnMut(&str)) -> Result<bool> {
        cmd.stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn command: {}", e))?;
        if let Some(stderr) = child.stderr.take() {
            for line in BufReader::new(stderr).lines() {
                match line {
                    Ok(l) => on_stderr(&l),
                    Err(_) => break,
                }
            }
        }
        let status = child
            .wait()
            .map_err(|e| anyhow!("Failed to wait for command: {}", e))?;
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
        cmd.arg("-R").arg(&self.repo_root).arg("--ignore-working-copy");
        cmd
    }

    fn log_cmd(&self, cmd: &Command) {
        tracing::debug!("Running jj: {:?}", cmd);
    }

    pub fn config_list(&self) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("config").arg("list");
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;

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
        if !self.runner.run_status(&mut cmd)? {
            return Err(anyhow!("jj config set failed"));
        }

        Ok(())
    }

    pub fn git_remote_list(&self) -> Result<String> {
        let mut cmd = self.cmd();
        cmd.arg("git").arg("remote").arg("list");
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;

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
        let output = self.runner.run_output(&mut cmd)?;

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
        let output = self.runner.run_output(&mut cmd)?;

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
        let output = self.runner.run_output(&mut cmd)?;
        if !output.status.success() {
            return Err(anyhow!(
                "jj bookmark set failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    pub fn describe(&self, change_id: &str, message: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("describe")
            .arg("-r")
            .arg(change_id)
            .arg("-m")
            .arg(message);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        if !output.status.success() {
            return Err(anyhow!(
                "jj describe failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
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
        let output = self.runner.run_output(&mut cmd)?;

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

    pub fn bookmark_track(&self, name: &str, remote: &str) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("bookmark")
            .arg("track")
            .arg(name)
            .arg("--remote")
            .arg(remote);
        self.log_cmd(&cmd);
        let output = self.runner.run_output(&mut cmd)?;
        if !output.status.success() {
            return Err(anyhow!(
                "jj bookmark track failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    pub fn git_push(
        &self,
        remote: &str,
        bookmark: &str,
        on_stderr: &mut dyn FnMut(&str),
    ) -> Result<()> {
        let mut cmd = self.cmd();
        cmd.arg("git")
            .arg("push")
            .arg("--remote")
            .arg(remote)
            .arg("--bookmark")
            .arg(bookmark);
        self.log_cmd(&cmd);
        if !self.runner.run_streaming(&mut cmd, on_stderr)? {
            return Err(anyhow!("jj git push failed"));
        }

        Ok(())
    }
}
