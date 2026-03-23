# Jellycat (`jc`) — AI Skills Reference

This document teaches AI assistants how to use `jc`, a CLI tool that bridges Jujutsu (`jj`) version control with GitHub Pull Requests.

## When to Use

Use `jc` instead of manual `gh` PR workflows whenever the repository is managed by Jujutsu (`jj`). Check for a `.jj/` directory to confirm.

## Prerequisites

- The repo must be initialized with `jc init` before any other commands work.
- `jj` and `gh` (GitHub CLI, authenticated) must be available on PATH.

## Commands

### `jc init [--force]`
Initialize jellycat for the current jj repo. Prompts for the upstream GitHub repo and push remote. Use `--force` to reconfigure.

### `jc submit [-r <REVSET>] [--stack]`
Push commits and create/update GitHub PRs. This is the primary workflow command.
- Without `-r`: submits the current commit (`@`)
- With `-r <REVSET>`: submits the specified revset
- With `--stack`: submits the full stack

### `jc status`
Show PR status (open/merged/closed, comment counts) for all tracked PRs.

### `jc link [-r <REVSET>] <PR_NUMBER> [--force]`
Manually associate a commit with an existing PR. Use when a PR was created outside `jc`.

### `jc unlink [-r <REVSET>] [PR_NUMBER]`
Remove a PR association from a commit.

### `jc tidy`
Clean up after merged/closed PRs: abandons completed changesets and removes config entries.

### `jc get <PR_NUMBER> [--checkout]`
Fetch a PR branch from GitHub. Use `--checkout` to also create a new jj working copy on it.

## Key Conventions

- **PR tracking**: `jc` stores PR associations as `PR: #NUM` lines in jj commit descriptions. Do not manually edit these lines.
- **Bookmarks**: `jc submit` automatically creates and manages jj bookmarks (prefixed `jellycat/` by default). Do not manually manage these.
- **Configuration**: Stored in jj's repo-local config under the `jellycat.*` namespace. Use `jj config set --repo` to modify if needed.

## Common Workflows

### Create a new PR
```bash
jj new -m "feat: add new feature"
# ... make changes ...
jc submit
```

### Submit a stack of PRs
```bash
jc submit --stack
```

### Check on PR status
```bash
jc status
```

### Clean up after PRs are merged
```bash
jc tidy
```

### Link an existing PR to a commit
```bash
jc link -r <revset> 123
```
