# Jellycat ("jc")

_A **jelly**fish copy**cat**_

This is a Jellyfish inspired CLI tool for managing [Jujutsu (`jj`)](https://github.com/martinvonz/jj) changesets with GitHub PRs. It aims to make sending stacks of changesets to GitHub easier.

## Prerequisites

- [Jujutsu (`jj`)](https://github.com/martinvonz/jj) with a colocated or native repo
- [GitHub CLI (`gh`)](https://cli.github.com/) authenticated via `gh auth login`
- Rust toolchain (to build from source)

## Installation

```bash
cargo install jellycat
```

Or if you want the latest,

```
cargo install --git https://github.com/fanzeyi/jellycat.git
```

## Quick Start

```bash
# Initialize in a jj repo
jc init

# Create a commit and submit it as a PR
jj new -m "my feature"
jc submit

# Check PR status
jc status

# Clean up after PRs are merged
jc tidy
```

## Demo

https://github.com/user-attachments/assets/850dfd89-0370-4652-8559-9cd5c801d176

<img width="1202" height="856" alt="Image" src="https://github.com/user-attachments/assets/9c6c54a8-9e47-4432-a407-4b560bbe053d" />

## Commands

### `jc init [--force]`

Initialize jellycat configuration for the current repository. Prompts for the upstream GitHub repo (`owner/repo`) and which git remote to push to.

Use `--force` to reconfigure an already-initialized repo.

### `jc submit [-r <REVSET>]`

Push commits and create or update GitHub PRs.

- `-r, --revset <REVSET>` — Revset to submit (default: `@`, the current commit)

This command:

1. Creates jj bookmarks for each commit
2. Pushes bookmarks to the configured remote
3. Creates new PRs or updates existing ones
4. Adds stack navigation links to PR bodies (for multi-PR stacks)

### `jc link [-r <REVSET>] <PR_NUMBER> [--force]`

Manually associate a commit with an existing GitHub PR.

- `-r, --revset <REVSET>` — Revset to link (default: `@`)
- `<PR_NUMBER>` — The PR number to associate
- `--force` — Overwrite an existing PR association

### `jc unlink [-r <REVSET>] [PR_NUMBER]`

Remove a PR association from a commit.

- `-r, --revset <REVSET>` — Revset to unlink (default: `@`)
- `[PR_NUMBER]` — Specific PR number to unlink (default: unlink all)

### `jc status`

Show the status of all tracked PRs, including their GitHub state (open/merged/closed), comment counts, and associated commit info.

### `jc tidy`

Clean up after merged, closed, or abandoned work:

- Removes config entries for changesets that have been abandoned in jj
- Abandons changesets linked to merged/closed PRs
- Removes all corresponding config entries

## Configuration

Configuration is stored in jj's repo-local config under the `jellycat.*` namespace. Most keys are set automatically by `init` and `submit`.

| Key                        | Description                                         |
| -------------------------- | --------------------------------------------------- |
| `jellycat.upstream`        | Target GitHub repo (`owner/repo`)                   |
| `jellycat.origin`          | Git remote name to push to                          |
| `jellycat.head_repo`       | Fork repo for cross-fork PRs (`owner/repo`)         |
| `jellycat.github_user`     | GitHub username for per-user token auth             |
| `jellycat.bookmark_prefix` | Prefix for created bookmarks (default: `jellycat/`) |
| `jellycat.prs.<change-id>` | PR number linked to a change ID                     |

Set config values with:

```bash
jj config set --repo jellycat.head_repo "myuser/myrepo"
```
