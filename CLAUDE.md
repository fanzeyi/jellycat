# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Jellycat is a Rust CLI tool (binary: `jc`) that bridges [Jujutsu (`jj`)](https://github.com/martinvonz/jj) version control with GitHub Pull Requests. It automates bookmark management, pushing to remotes, and PR creation/updates.

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

- **`src/gh.rs`** — `Gh` client wrapping all GitHub CLI (`gh`) interactions. Uses `CommandRunner` like `Jj` for testability. Supports per-user token auth via `GH_TOKEN` env var. The `JELLYCAT_GH_BINARY` env var overrides the `gh` binary path.

- **`src/commands/`** — Each subcommand (`init`, `submit`, `link`, `unlink`, `status`, `tidy`, `get`) lives in its own module with a `run()` function. The `Commands` enum in `mod.rs` wires them to clap.

- **`src/commands/context.rs`** — `CommandCtx` bundles per-command bootstrap state (`repo_root`, `Arc<Jj>`, shared `Arc<dyn CommandRunner>`) and helpers (`gh`, `gh_with_auth`, `require_upstream`). Every command's `run()` should start with `CommandCtx::new()?` instead of repeating `find_root`/`Jj::new`/gh-auth boilerplate. Fields are `pub` so tests can construct `CommandCtx` directly, bypassing `find_root`.

- **`src/config.rs`** — Reads/writes jellycat config stored in jj's repo-local config under the `jellycat.*` namespace. Config keys are declared as `const`s in the `config::keys` submodule — never hardcode `"jellycat.*"` strings at call sites. The loader is split into `load()` (I/O) + `load_from_entries()` (pure), so parsing is unit-testable.

- **`src/pr_store.rs`** — `PrStore` trait with two backends (`ConfigPrStore`, `BookmarkPrStore`) for PR ↔ change-id mappings. Passed into commands as `&dyn PrStore`.

- **`src/repo.rs`** — Utilities: `find_root()` walks up to find `.jj/`, `get_single_commit()` fetches a commit as `JjLogCommit`.

### Key Conventions

- PR associations are stored via the configured `PrStore` backend (config keys or `pr-<NUM>` bookmarks), keyed by change-id. Do not parse `PR: #NUM` from commit descriptions.
- Use `eyre::Result<()>` (with `color-eyre` for display) for fallible functions. Do not use `anyhow`.
- All subprocess execution goes through the `CommandRunner` trait. Never call `Command::new(...).output()/status()` directly in command modules — add a method on `Jj` or `Gh` (or extend `CommandCtx`) so it's mockable.
- Commands start with `let ctx = CommandCtx::new()?;` and use `ctx.jj`, `ctx.gh(config)?`, `ctx.require_upstream(config)?` rather than wiring these up manually.
- Long phase-style commands (e.g. `submit`) should be split into small private phase functions rather than one giant `run` body — each phase takes explicit inputs, returns explicit outputs.
- Config key strings live in `config::keys`; reference constants (e.g. `keys::UPSTREAM_REPO`) instead of string literals.
- Prefer JSON output mode when parsing command outputs (`jj log -T json`, `gh` JSON flags).
- When reading `jj` help, pass `--no-pager` to avoid a pager being started.

### Testing Pattern

Integration tests in `tests/submit_test.rs` and `tests/stack_navigation_test.rs` use `mockall` to mock `CommandRunner`, `tempfile` for temporary repos, and `assert_cmd`/`predicates` for CLI assertions. Mock the `CommandRunner` trait to test command logic without invoking real `jj` or `gh` binaries.

When testing a command, construct `CommandCtx` directly with the mock runner (its fields are `pub`) rather than calling `CommandCtx::new()`, so the test doesn't need a real `.jj` directory. For pure parsing logic (e.g. `config::load_from_entries`), prefer plain `#[cfg(test)] mod tests` unit tests that feed canned input — no mocking needed.
