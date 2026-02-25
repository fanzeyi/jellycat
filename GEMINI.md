# Jellycat

Jellycat is a CLI tool designed to bridge the gap between Jujutsu (`jj`)'s local-first workflow and GitHub's Pull Request-based model. It facilitates the process of submitting local changes as GitHub PRs by automating bookmark management and pushing to remote git repositories.

## Project Overview

- **Primary Technology**: Rust (2024 edition).
- **Core Dependencies**:
  - `clap`: For CLI argument parsing.
  - `anyhow`: For robust error handling.
  - `serde` & `serde_json`: For JSON serialization/deserialization (specifically for `jj` and `gh` outputs).
- **Architecture**:
  - **Subcommands**: Command logic is modularized in `src/commands/` (e.g., `init`, `submit`, `link`, `unlink`, `status`).
  - **JJ Client**: A centralized `Jj` client in `src/jj.rs` handles all interactions with the `jj` binary, ensuring consistent use of repository flags (`-R`).
  - **Configuration**: Jellycat stores its configuration within the `jj` repository-local configuration under the `jellycat.` namespace (e.g., `jellycat.upstream`, `jellycat.origin`).

## Key Subcommands

- `init`: Configures the upstream GitHub repository (e.g., `owner/repo`) and the git remote to use as `origin`.
- `submit`: Pushes the specified revset (defaulting to `@`) to the remote and prepares it for PR creation/update. It automatically generates bookmark names if no PR is linked.
- `link`: Associates a specific PR number with a commit description (adds `PR: #NUM` to the description).
- `unlink`: Removes PR associations from a commit description.
- `status`: (Placeholder) Displays current configuration and status.

## Building and Running

- **Build**: `cargo build`
- **Run**: `cargo run -- [subcommand] [args]`
- **Test**: `cargo test` (Currently using default Rust test harness)
- **Check**: `cargo check` for syntax and type verification.

## Development Conventions

- **Error Handling**: Use `anyhow::Result<()>` for functions that can fail, especially in the `run` methods of subcommands.
- **Command Execution**: Use the `Jj` client for any Jujutsu-related operations. For GitHub operations, use `gh` CLI via `std::process::Command`.
- **Commit Descriptions**: Jellycat relies on `PR: #NUM` markers in commit descriptions to track Pull Request associations.
- **Code Style**: Follow standard idiomatic Rust conventions (`cargo fmt`).
- **JSON Communication**: Prefer to use JSON if the command we use offers it. It's more reliable when parsing outputs.

## Testing

- There is a test repository you can use under `/Users/zeyi/Code/test-fork`.
- The repository's remote is my own fork. It is a fork of `zerayrice/exp`. You can use both repositories to test.

## Tips

- If you want to read `jj` help, you must pass `--no-pager `flag otherwise it will start a pager for you.
