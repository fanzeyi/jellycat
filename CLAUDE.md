# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Jellycat is a Rust CLI tool that bridges [Jujutsu (`jj`)](https://github.com/martinvonz/jj) version control with GitHub Pull Requests. It automates bookmark management, pushing to remotes, and PR creation/updates.

## Commands

```bash
cargo build          # Build the project
cargo run -- [args]  # Run with subcommands
cargo test           # Run all tests
cargo test <name>    # Run a single test by name
cargo check          # Type-check without building
cargo fmt            # Format code
cargo clippy         # Lint
```

There is a test repository at `/Users/zeyi/Code/test-fork` (a fork of `zerayrice/exp`) available for manual testing.

## Architecture

### Core Components

- **`src/jj.rs`** — Centralized `Jj` client wrapping all `jj` binary interactions. Always passes `-R <repo_root>` to ensure commands target the right repo. The `CommandRunner` trait (`run_output`, `run_status`) abstracts process execution for testability; `DefaultRunner` is the real implementation used in production.

- **`src/commands/`** — Each subcommand (`init`, `submit`, `link`, `unlink`, `status`) lives in its own module with a `run()` function. The `Commands` enum in `mod.rs` wires them to clap.

- **`src/config.rs`** — Reads/writes jellycat config stored in jj's repo-local config under the `jellycat.*` namespace (`jellycat.upstream`, `jellycat.origin`).

- **`src/repo.rs`** — Utilities: `find_root()` walks up to find `.jj/`, `get_single_commit()` fetches a commit as `JjLogCommit`.

### Key Conventions

- PR associations are stored in commit descriptions as `PR: #NUM` lines. `submit` adds this automatically; `link`/`unlink` manage it manually.
- Use `anyhow::Result<()>` for fallible functions.
- Use the `Jj` client for all jj operations; use `gh` CLI via `std::process::Command` for GitHub operations.
- Prefer JSON output mode when parsing command outputs (`jj log -T json`, `gh` JSON flags).
- When reading `jj` help, pass `--no-pager` to avoid a pager being started.

### Testing Pattern

Integration tests in `tests/submit_test.rs` use `mockall` to mock `CommandRunner`, `tempfile` for temporary repos, and `assert_cmd`/`predicates` for CLI assertions. Mock the `CommandRunner` trait to test command logic without invoking real `jj` or `gh` binaries.
