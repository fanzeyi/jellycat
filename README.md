# Jellycat

A CLI tool that bridges [Jujutsu (`jj`)](https://github.com/martinvonz/jj) version control with GitHub Pull Requests. It automates bookmark management, pushing to remotes, and PR creation/updates — including stacked PR workflows.

## Prerequisites

- [Jujutsu (`jj`)](https://github.com/martinvonz/jj) with a colocated or native repo
- [GitHub CLI (`gh`)](https://cli.github.com/) authenticated via `gh auth login`
- Rust toolchain (to build from source)

## Installation

```bash
cargo install --path .
```

## Quick Start

```bash
# Initialize in a jj repo
jellycat init

# Create a commit and submit it as a PR
jj new -m "my feature"
jellycat submit

# Check PR status
jellycat status

# Clean up after PRs are merged
jellycat tidy
```

## Commands

### `jellycat init [--force]`

Initialize jellycat configuration for the current repository. Prompts for the upstream GitHub repo (`owner/repo`) and which git remote to push to.

Use `--force` to reconfigure an already-initialized repo.

### `jellycat submit [-r <REVSET>]`

Push commits and create or update GitHub PRs.

- `-r, --revset <REVSET>` — Revset to submit (default: `@`, the current commit)

This command:
1. Creates jj bookmarks for each commit
2. Pushes bookmarks to the configured remote
3. Creates new PRs or updates existing ones
4. Adds stack navigation links to PR bodies (for multi-PR stacks)

### `jellycat link [-r <REVSET>] <PR_NUMBER> [--force]`

Manually associate a commit with an existing GitHub PR.

- `-r, --revset <REVSET>` — Revset to link (default: `@`)
- `<PR_NUMBER>` — The PR number to associate
- `--force` — Overwrite an existing PR association

### `jellycat unlink [-r <REVSET>] [PR_NUMBER]`

Remove a PR association from a commit.

- `-r, --revset <REVSET>` — Revset to unlink (default: `@`)
- `[PR_NUMBER]` — Specific PR number to unlink (default: unlink all)

### `jellycat status`

Show the status of all tracked PRs, including their GitHub state (open/merged/closed), comment counts, and associated commit info.

### `jellycat tidy`

Clean up after merged, closed, or abandoned work:
- Removes config entries for changesets that have been abandoned in jj
- Abandons changesets linked to merged/closed PRs
- Removes all corresponding config entries

## Configuration

Configuration is stored in jj's repo-local config under the `jellycat.*` namespace. Most keys are set automatically by `init` and `submit`.

| Key | Description | Set by |
|-----|-------------|--------|
| `jellycat.upstream` | Target GitHub repo (`owner/repo`) | `init` |
| `jellycat.origin` | Git remote name to push to | `init` |
| `jellycat.head_repo` | Fork repo for cross-fork PRs (`owner/repo`) | manual |
| `jellycat.github_user` | GitHub username for per-user token auth | manual |
| `jellycat.bookmark_prefix` | Prefix for created bookmarks (default: `jellycat/`) | manual |
| `jellycat.prs.<change-id>` | PR number linked to a change ID | `submit`, `link`, `tidy` |

Set config values with:

```bash
jj config set --repo jellycat.head_repo "myuser/myrepo"
```

## Global Flag

- `--debug` / `-d` — Enable debug logging to stderr

## Development

```bash
cargo build          # Build
cargo test           # Run tests
cargo check          # Type-check
cargo fmt            # Format
cargo clippy         # Lint
```
