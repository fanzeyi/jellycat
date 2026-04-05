use std::fmt;
use std::process::Command;

use color_eyre::{Section, SectionExt};

#[derive(Debug)]
pub struct CommandError {
    pub command: String,
    pub exit_code: Option<i32>,
    pub stderr: String,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Command exeuction failed")
    }
}

impl std::error::Error for CommandError {}

impl CommandError {
    fn into_report(self) -> eyre::Report {
        let command_section = if let Some(exit_code) = self.exit_code {
            format!("{}\n(exit code: {})", self.command, exit_code)
        } else {
            self.command.clone()
        };
        let stderr_section = self.stderr.clone();
        eyre::Report::from(self)
            .section(stderr_section.header("Stderr:"))
            .section(command_section.header("Command:"))
    }
}

pub fn format_cmd(cmd: &Command) -> String {
    let program = cmd.get_program().to_string_lossy();
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if args.is_empty() {
        program.into_owned()
    } else {
        format!("{} {}", program, args.join(" "))
    }
}

pub fn check_output(cmd_str: String, output: &std::process::Output) -> eyre::Result<()> {
    if output.status.success() {
        return Ok(());
    }
    Err(CommandError {
        command: cmd_str,
        exit_code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    }
    .into_report())
}

pub fn format_error_short(err: &eyre::Report) -> String {
    if let Some(command_error) = err.downcast_ref::<CommandError>() {
        format!("Command execution failed: {}", command_error.stderr)
    } else {
        err.to_string()
    }
}
