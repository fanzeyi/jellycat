/// Integration test: submitting a fresh two-commit stack produces correct
/// navigation links in each PR body.
///
/// Real `jj` is used to build the repo; `gh` is replaced by a Python fake
/// (via JELLYCAT_GH_BINARY) that records every PR-creation body to a file.
/// `jellycat` itself runs as a subprocess so env vars are scoped to the child.
use assert_cmd::Command;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::tempdir;

/// Fake `gh` binary (Python 3, self-contained).
///
/// State directory: $JELLYCAT_GH_STATE_DIR
///   pr_count          — running PR counter
///   pr_body_<N>       — body passed to the Nth create-PR call
///   pr_headref_<N>    — head ref for PR N
///   pr_body_edit_<N>  — body passed to a pr-edit call
const FAKE_GH: &str = r#"#!/usr/bin/env python3
import sys, os, json

args = sys.argv[1:]
state = os.environ.get('JELLYCAT_GH_STATE_DIR', '/tmp/gh_fake_state')
os.makedirs(state, exist_ok=True)

cmd = args[0] if args else ''

# ── auth status ───────────────────────────────────────────────────────────────
if cmd == 'auth' and len(args) > 1 and args[1] == 'status':
    print(json.dumps({
        'hosts': {'github.com': [{'login': 'testuser', 'token': 'testtoken'}]}
    }))
    sys.exit(0)

# ── api ───────────────────────────────────────────────────────────────────────
if cmd == 'api':
    # Create PR
    if '--method' in args and 'POST' in args:
        body = ''
        head = ''
        i = 0
        while i < len(args):
            if args[i] == '-f' and i + 1 < len(args):
                key, _, value = args[i + 1].partition('=')
                if key == 'body':
                    body = value
                elif key == 'head':
                    head = value
                i += 2
            else:
                i += 1

        count_file = os.path.join(state, 'pr_count')
        pr_num = int(open(count_file).read().strip()) + 1 if os.path.exists(count_file) else 1
        open(count_file, 'w').write(str(pr_num))

        open(os.path.join(state, f'pr_body_{pr_num}'), 'w').write(body)
        headref = head.split(':', 1)[-1] if ':' in head else head
        open(os.path.join(state, f'pr_headref_{pr_num}'), 'w').write(headref)

        print(json.dumps({
            'number': pr_num,
            'html_url': f'https://github.com/owner/repo/pull/{pr_num}'
        }))
        sys.exit(0)

    # Default branch
    for i, arg in enumerate(args):
        if arg == '--jq' and i + 1 < len(args) and args[i + 1] == '.default_branch':
            print('main')
            sys.exit(0)

# ── pr ────────────────────────────────────────────────────────────────────────
if cmd == 'pr':
    subcmd = args[1] if len(args) > 1 else ''
    pr_num = int(args[2]) if len(args) > 2 else 0

    if subcmd == 'view' and '--json' in args:
        field = args[args.index('--json') + 1]
        if field == 'headRefName':
            hf = os.path.join(state, f'pr_headref_{pr_num}')
            headref = open(hf).read().strip() if os.path.exists(hf) else f'branch{pr_num}'
            print(json.dumps({'headRefName': headref}))
            sys.exit(0)
        if field == 'body':
            bf = os.path.join(state, f'pr_body_{pr_num}')
            body = open(bf).read() if os.path.exists(bf) else ''
            print(json.dumps({'body': body}))
            sys.exit(0)

    if subcmd == 'edit':
        for i, arg in enumerate(args):
            if arg == '--body' and i + 1 < len(args):
                open(os.path.join(state, f'pr_body_edit_{pr_num}'), 'w').write(args[i + 1])
                break
        sys.exit(0)

print(f'Unhandled gh command: {args}', file=sys.stderr)
sys.exit(1)
"#;

fn jj(repo: &Path, args: &[&str]) {
    let status = std::process::Command::new("jj")
        .arg("-R")
        .arg(repo)
        .args(args)
        .status()
        .expect("jj not found");
    assert!(status.success(), "jj {args:?} failed");
}

fn setup_repo(base: &Path) -> std::path::PathBuf {
    let repo = base.join("repo");
    let remote = base.join("remote.git");

    let s = std::process::Command::new("git")
        .args(["init", "--bare", remote.to_str().unwrap()])
        .status()
        .expect("git not found");
    assert!(s.success());

    let s = std::process::Command::new("jj")
        .args(["git", "init", repo.to_str().unwrap()])
        .status()
        .expect("jj not found");
    assert!(s.success(), "jj git init failed");

    jj(&repo, &["git", "remote", "add", "origin", remote.to_str().unwrap()]);
    jj(&repo, &["config", "set", "--repo", "user.name", "Test"]);
    jj(&repo, &["config", "set", "--repo", "user.email", "test@test.com"]);
    // Jellycat config (what `jellycat init` would write)
    jj(&repo, &["config", "set", "--repo", "jellycat.upstream", "owner/repo"]);
    jj(&repo, &["config", "set", "--repo", "jellycat.origin", "origin"]);

    repo
}

#[test]
fn test_stack_pr_navigation_for_new_prs() {
    let tmp = tempdir().unwrap();
    let repo = setup_repo(tmp.path());
    let state_dir = tmp.path().join("gh_state");

    // Build a two-commit stack: root → A → B (@)
    jj(&repo, &["describe", "-m", "feat: commit A"]);
    jj(&repo, &["new", "-m", "feat: commit B"]);

    // Write the fake gh script
    let fake_gh = tmp.path().join("fake_gh");
    fs::write(&fake_gh, FAKE_GH).unwrap();
    let mut perms = fs::metadata(&fake_gh).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake_gh, perms).unwrap();

    // Run jellycat as a subprocess — env vars are scoped to the child process.
    // CARGO_BIN_EXE_jellycat is set by cargo to the path of the compiled binary.
    Command::new(env!("CARGO_BIN_EXE_jellycat"))
        .current_dir(&repo)
        .env("JELLYCAT_GH_BINARY", &fake_gh)
        .env("JELLYCAT_GH_STATE_DIR", &state_dir)
        .args(["submit", "-r", "@-::@"])
        .assert()
        .success();

    // PR 1 = commit A (submitted first; B has no PR yet at that point)
    let pr1_body = fs::read_to_string(state_dir.join("pr_body_1"))
        .expect("PR 1 body file not written");
    // PR 2 = commit B (submitted second; pr_map must carry PR #1 from A)
    let pr2_body = fs::read_to_string(state_dir.join("pr_body_2"))
        .expect("PR 2 body file not written — navigation bug may still be present");

    assert!(
        !pr1_body.contains("Next PR"),
        "PR 1 should have no next-PR link (B not yet submitted at that point)\nbody:\n{pr1_body}"
    );
    assert!(
        pr2_body.contains("Previous PR") && pr2_body.contains("/pull/1"),
        "PR 2 body must contain a navigation link back to PR #1\nbody:\n{pr2_body}"
    );
}
