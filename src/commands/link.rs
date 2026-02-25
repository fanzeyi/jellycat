use crate::repo;
use crate::jj::Jj;
use anyhow::{anyhow, Result, Context};
use clap::Args;

#[derive(Args, Debug)]
pub struct LinkArgs {
    /// The revset to link a PR to (must resolve to a single commit)
    #[arg(short = 'r', long = "revset", default_value = "@")]
    pub revset: String,

    /// The PR number to link
    pub pr_number: u32,

    /// Overwrite existing PR link
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: &LinkArgs) -> Result<()> {
    let repo_root = repo::find_root()
        .ok_or_else(|| anyhow!("Not a jujutsu repository (or any of the parent directories): .jj"))?;

    let jj = Jj::new(repo_root.clone());

    // 1. Get the commit.
    let commit = repo::get_single_commit(&repo_root, &args.revset)
        .context("Failed to get commit")?;

    // 2. Check for existing PRs and filter them out if --force is used.
    let mut new_description_lines = Vec::new();
    let mut already_linked = false;
    for line in commit.description.lines() {
        if let Some(pr_num_str) = line.trim().strip_prefix("PR: #") {
            if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                if !args.force {
                    if pr_num == args.pr_number {
                        println!(
                            "PR #{} is already linked to changeset {}.",
                            args.pr_number, commit.change_id
                        );
                        already_linked = true;
                    } else {
                        return Err(anyhow!(
                            "Commit is already linked to PR #{}. Use --force to overwrite.",
                            pr_num
                        ));
                    }
                }
            }
        } else {
            new_description_lines.push(line);
        }
    }

    if already_linked {
        return Ok(());
    }

    // 3. Construct the new description.
    let mut new_description = new_description_lines.join("\n").trim_end().to_string();
    if !new_description.is_empty() {
        new_description.push_str("\n\n");
    }
    new_description.push_str(&format!("PR: #{}", args.pr_number));

    // 4. Update commit description.
    jj.describe(&args.revset, &new_description)
        .context("jj describe failed")?;

    println!(
        "Linked PR #{} to change {}",
        args.pr_number, commit.change_id
    );

    Ok(())
}
