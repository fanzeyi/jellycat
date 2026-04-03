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

### Shell Completions

```bash
# Bash
jc completions bash > ~/.local/share/bash-completion/completions/jc

# Zsh
jc completions zsh > ~/.zfunc/_jc

# Fish
jc completions fish > ~/.config/fish/completions/jc.fish
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

## Integrations

### Show PR Numbers in `jj log`

Jellycat stores changeset to PR mapping in `jj config`. This allows you to look up the PR number of your changeset directly in JJ's templating language.

However, dynamic config lookup is not supported in JJ until 0.40.0 (https://github.com/jj-vcs/jj/pull/9148).

Add these template function to your JJ's `config.toml`

```toml
[template-aliases]
'pr_id(commit)' = 'config("jellycat.prs." ++ commit.change_id())'
'pr_url(commit)' = 'concat("https://github.com/", config("jellycat.upstream_repo").as_string(), "/pull/", pr_id(commit))'
'format_pr(commit)' = '''
if(pr_id(commit),
  label("jellycat_pr_link",
    hyperlink(pr_url(commit),
      "#" ++ pr_id(commit),
      concat("#", pr_id(commit), " ", pr_url(commit))
    )
  )
)
'''
```

You can customize the color of `format_pr()` with `colors.jellycat_pr_link`:

```toml
[colors]
jellycat_pr_link = "yellow"
```

I elected to overwrite the builtin `format_short_commit_header(commit)` to show PR link in my `jj log` default output. This is not the best practice but I do not want to rewrite the entire `builtin_log_compact` template yet:

```toml
'format_short_commit_header(commit)' = '''
separate(" ",
  format_short_change_id_with_change_offset(commit),
  format_short_signature(commit.author()),
  format_timestamp(commit_timestamp(commit)),
  commit.bookmarks(),
  commit.tags(),
  commit.working_copies(),
  format_short_commit_id(commit.commit_id()),
  format_commit_labels(commit),
  format_pr(commit),
  if(config("ui.show-cryptographic-signatures").as_boolean(),
    format_short_cryptographic_signature(commit.signature())
  ),
)
'''
```

Preview:

<img width="679" height="297" alt="Image" src="https://github.com/user-attachments/assets/66f04852-3fb9-4802-a612-d63d66959bbc" />

## Commands

### `jc init [--force]`

Initialize jellycat configuration for the current repository. Prompts for the upstream GitHub repo (`owner/repo`) and which git remote to push to.

Use `--force` to reconfigure an already-initialized repo.

### `jc submit [-r <REVSET>]`

Push commits and create or update GitHub PRs.

- `-r, --revset <REVSET>` — Revset to submit (default: `@`, the current commit)
- `-d, --draft` — Create new PRs as draft (does not affect existing PRs)

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

## AI Assistant Integration

Teach your AI coding assistant (Claude Code, Cursor, etc.) how to use `jc`:

```bash
jc skills >> ~/.claude/CLAUDE.md       # Claude Code
jc skills >> .cursor/rules/jc.md       # Cursor
```

## Configuration

Configuration is stored in jj's repo-local config under the `jellycat.*` namespace. Most keys are set automatically by `init` and `submit`.

| Key                        | Description                                         |
| -------------------------- | --------------------------------------------------- |
| `jellycat.upstream`        | Git remote name for the upstream repository         |
| `jellycat.upstream_repo`   | Upstream GitHub repo (`owner/repo`)                 |
| `jellycat.origin`          | Git remote name to push to                          |
| `jellycat.origin_repo`     | Fork repo for cross-fork PRs (`owner/repo`)         |
| `jellycat.github_user`     | GitHub username for per-user token auth             |
| `jellycat.bookmark_prefix` | Prefix for created bookmarks (default: `jellycat/`) |
| `jellycat.draft`           | Create new PRs as draft by default (`true`/`false`) |
| `jellycat.default_revset`  | Default JJ revset to use when submitting            |
| `jellycat.prs.<change-id>` | PR number linked to a change ID                     |

Set config values with:

```bash
jj config set --repo jellycat.origin_repo "myuser/myrepo"
```
