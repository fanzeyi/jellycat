use eyre::Result;
use jellycat::commands::CommandCtx;
use jellycat::commands::link::{LinkArgs, run_smart};
use jellycat::config::Config;
use jellycat::gh::Gh;
use jellycat::jj::{CommandRunner, Jj};
use jellycat::pr_store;
use mockall::mock;
use std::collections::HashMap;
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
fn test_link_smart_links_matching_bookmark() {
    let mut mock_runner = MockMyRunner::new();
    let temp_dir = tempdir().unwrap();

    // 1. gh pr list — returns one open PR whose head ref matches a local bookmark.
    mock_runner
        .expect_run_output()
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("pr"))
                && args.contains(&std::ffi::OsStr::new("list"))
                && args.contains(&std::ffi::OsStr::new("@me"))
        })
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: br#"[{"number":42,"headRefName":"feat/foo"},{"number":43,"headRefName":"no-such-bookmark"}]"#.to_vec(),
                stderr: Vec::new(),
            })
        });

    // 2. jj bookmark list
    mock_runner
        .expect_run_output()
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("bookmark"))
                && args.contains(&std::ffi::OsStr::new("list"))
        })
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: b"feat/foo\tabc123abc123\nother\tdef456def456\n".to_vec(),
                stderr: Vec::new(),
            })
        });

    // 3. jj log for feat/foo — resolves to a change_id.
    mock_runner
        .expect_run_output()
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("log"))
                && args.contains(&std::ffi::OsStr::new("feat/foo"))
        })
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: br#"{"change_id":"change_foo_id","description":"feat: foo"}"#.to_vec(),
                stderr: Vec::new(),
            })
        });

    // 4. jj config set — PrStore.set for the newly linked PR.
    mock_runner
        .expect_run_output()
        .times(1)
        .withf(|cmd| {
            let args: Vec<_> = cmd.get_args().collect();
            args.contains(&std::ffi::OsStr::new("config"))
                && args.contains(&std::ffi::OsStr::new("set"))
                && args
                    .iter()
                    .any(|a| a.to_string_lossy().contains("change_foo_id"))
        })
        .returning(|_| {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        });

    let runner: Arc<dyn CommandRunner + Send + Sync> = Arc::new(mock_runner);

    let config = Config {
        upstream_repo: Some("owner/repo".to_string()),
        origin: Some("origin".to_string()),
        prs: HashMap::new(),
        deprecated_keys: vec![],
        ..Default::default()
    };

    let jj = Arc::new(Jj::with_runner(
        temp_dir.path().to_path_buf(),
        Arc::clone(&runner),
    ));
    let store = pr_store::create(&config.pr_store_type, Arc::clone(&jj));

    let cmd = CommandCtx {
        repo_root: temp_dir.path().to_path_buf(),
        jj: Arc::clone(&jj),
        runner: Arc::clone(&runner),
    };

    let gh = Gh::with_token(Arc::clone(&runner), "fake-token".to_string());

    let args = LinkArgs {
        revset: "@".to_string(),
        pr_number: None,
        force: false,
        smart: true,
    };

    run_smart(&args, &config, store.as_ref(), &cmd, &gh, "owner/repo").unwrap();
}
