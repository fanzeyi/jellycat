use anyhow::Result;
use jellycat::commands::submit::{SubmitArgs, SubmitContext, run_with_context};
use jellycat::config::Config;
use jellycat::jj::CommandRunner;
use mockall::mock;
use std::collections::HashMap;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};
use std::sync::Arc;
use tempfile::tempdir;

mock! {
    pub MyRunner {}
    impl CommandRunner for MyRunner {
        fn run_output(&self, cmd: &mut std::process::Command) -> Result<Output>;
        fn run_status(&self, cmd: &mut std::process::Command) -> Result<bool>;
    }
}

#[test]
fn test_submit_auth_failure() {
    let mut mock_runner = MockMyRunner::new();

    mock_runner.expect_run_output().returning(|_| {
        Ok(Output {
            status: ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: b"not logged in".to_vec(),
        })
    });

    let config = Config {
        upstream: Some("owner/repo".to_string()),
        origin: Some("origin".to_string()),
        head_repo: None,
        github_user: None,
        prs: HashMap::new(),
        bookmark_prefix: None,
    };

    let ctx = SubmitContext {
        config: &config,
        runner: Arc::new(mock_runner),
    };

    let args = SubmitArgs {
        revset: Some("@".to_string()),
        stack: false,
    };

    let result = run_with_context(&args, &ctx);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("gh auth status failed")
    );
}

#[test]
fn test_submit_success_new_pr() {
    let mut mock_runner = MockMyRunner::new();
    let temp_dir = tempdir().unwrap();
    let jj_dir = temp_dir.path().join(".jj");
    fs::create_dir(&jj_dir).unwrap();

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // 1. gh auth status
    mock_runner
        .expect_run_output()
        .withf(|cmd| cmd.get_args().any(|a| a == "status"))
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: br#"{"hosts":{"github.com":[{"login":"testuser","token":"testtoken"}]}}"#
                    .to_vec(),
                stderr: Vec::new(),
            })
        });

    // 2. jj log_reversed (main loop)
    mock_runner.expect_run_output()
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("log")) && args.contains(&std::ffi::OsStr::new("--reversed")) && args.contains(&std::ffi::OsStr::new("@"))
        })
        .returning(|_| Ok(Output {
            status: ExitStatus::from_raw(0),
            stdout: br#"{"commit_id":"commit1_full_id_here","change_id":"change1_full_id_here","description":"feat: first commit","parents":["parent1"]}"#.to_vec(),
            stderr: Vec::new(),
        }));

    // 3. jj get_stack (now queries by change_id, not commit_id)
    mock_runner.expect_run_output()
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("log")) && args.contains(&std::ffi::OsStr::new("(::change1_full_id_here | change1_full_id_here::) & mutable()"))
        })
        .returning(|_| Ok(Output {
            status: ExitStatus::from_raw(0),
            stdout: br#"{"commit_id":"commit1_full_id_here","change_id":"change1_full_id_here","description":"feat: first commit","parents":["parent1"]}"#.to_vec(),
            stderr: Vec::new(),
        }));

    // 4. gh api get default branch
    mock_runner
        .expect_run_output()
        .withf(|cmd| cmd.get_args().any(|a| a == "api") && cmd.get_args().any(|a| a == "--jq"))
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: b"main".to_vec(),
                stderr: Vec::new(),
            })
        });

    // 5. jj bookmark_set — called once (no Phase 7 re-point)
    mock_runner
        .expect_run_output()
        .times(1)
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("bookmark"))
                && args.contains(&std::ffi::OsStr::new("set"))
        })
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        });

    // 6. jj bookmark_track
    mock_runner
        .expect_run_output()
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("bookmark"))
                && args.contains(&std::ffi::OsStr::new("track"))
        })
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        });

    // 7. jj git push — only Phase 3 (single push, no Phase 7)
    mock_runner
        .expect_run_status()
        .withf(|cmd| cmd.get_args().any(|a| a == "push"))
        .returning(|_| Ok(true));

    // 8. gh api create PR (Phase 4)
    mock_runner
        .expect_run_output()
        .withf(|cmd| cmd.get_args().any(|a| a == "api") && cmd.get_args().any(|a| a == "POST"))
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: br#"{"number":123,"html_url":"https://github.com/owner/repo/pull/123"}"#
                    .to_vec(),
                stderr: Vec::new(),
            })
        });

    // 9. gh pr edit body (Phase 5 — updates all PR bodies with stack graph)
    mock_runner
        .expect_run_output()
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("pr"))
                && args.contains(&std::ffi::OsStr::new("edit"))
        })
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        });

    // 10. jj config set (save PR mapping to config)
    mock_runner
        .expect_run_status()
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("config"))
                && args.contains(&std::ffi::OsStr::new("set"))
        })
        .returning(|_| Ok(true));

    let config = Config {
        upstream: Some("owner/repo".to_string()),
        origin: Some("origin".to_string()),
        head_repo: None,
        github_user: None,
        prs: HashMap::new(),
        bookmark_prefix: None,
    };

    let ctx = SubmitContext {
        config: &config,
        runner: Arc::new(mock_runner),
    };

    let args = SubmitArgs {
        revset: Some("@".to_string()),
        stack: false,
    };

    let result = run_with_context(&args, &ctx);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "Submit failed: {:?}", result.err());
}
