# CLAUDE.md

## Overview

Jellycat: Rust CLI (`jc`) bridging [Jujutsu (`jj`)](https://github.com/martinvonz/jj) with GitHub PRs. Automates bookmarks, pushes, PR create/update.

## Commands

`cargo build | test [name] | check | fmt | clippy`. Test repo: `/Users/zeyi/Code/test-fork`.

## Architecture

- **`src/jj.rs`** — `Jj` client wrapping `jj` binary; always passes `-R <repo_root>`. `CommandRunner` trait (`run_output`/`run_status`) abstracts process exec; `DefaultRunner` is production impl.
- **`src/gh.rs`** — `Gh` client wrapping `gh`. Uses `CommandRunner`. Per-user auth via `GH_TOKEN`; `JELLYCAT_GH_BINARY` overrides binary.
- **`src/commands/`** — Each subcommand (`init`, `submit`, `link`, `unlink`, `status`, `tidy`, `get`) is a module with `run()`. `Commands` enum in `mod.rs` wires clap.
- **`src/commands/context.rs`** — `CommandCtx` bundles `repo_root`, `Arc<Jj>`, `Arc<dyn CommandRunner>` + helpers (`gh`, `gh_with_auth`, `require_upstream`). Fields `pub` so tests construct directly.
- **`src/config.rs`** — Config in jj's repo-local config under `jellycat.*`. Keys as `const`s in `config::keys` — never hardcode strings. Split `load()` (I/O) + `load_from_entries()` (pure, unit-testable).
- **`src/pr_store.rs`** — `PrStore` trait, backends `ConfigPrStore`/`BookmarkPrStore` for PR↔change-id mapping. Passed as `&dyn PrStore`.
- **`src/repo.rs`** — `find_root()` walks up for `.jj/`; `get_single_commit()` returns `JjLogCommit`.

## Conventions

- PR associations via `PrStore` keyed by change-id; don't parse `PR: #NUM` from descriptions.
- Use `eyre::Result` + `color-eyre`. No `anyhow`.
- All subprocess exec through `CommandRunner` — never `Command::new().output()/status()` in command modules. Add methods on `Jj`/`Gh`/`CommandCtx`.
- Commands start with `let ctx = CommandCtx::new()?;` then use `ctx.jj`, `ctx.gh(config)?`, `ctx.require_upstream(config)?`.
- Split phase-style commands (e.g. `submit`) into small private phase fns with explicit I/O.
- Reference `keys::*` constants, not string literals.
- Prefer JSON output (`jj log -T json`, `gh` JSON flags).
- Pass `--no-pager` when reading `jj` help.

## Testing

Integration tests (`tests/submit_test.rs`, `tests/stack_navigation_test.rs`) use `mockall` on `CommandRunner`, `tempfile`, `assert_cmd`/`predicates`. Construct `CommandCtx` directly with mock runner (pub fields) to skip real `.jj`. For pure parsing (e.g. `config::load_from_entries`), use `#[cfg(test)] mod tests` with canned input — no mocking.
