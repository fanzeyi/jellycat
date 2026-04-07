use clap::{CommandFactory, Parser};
use clap_complete::generate;
use color_eyre::eyre::Result;
use eyre::{Context, eyre};
use jellycat::commands::Commands;
use jellycat::config;
use jellycat::jj::Jj;
use jellycat::repo;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "jc",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_HASH"), ")"),
    about,
    long_about = None,
)]
struct Cli {
    #[arg(short, long, global = true)]
    debug: bool,
    #[command(subcommand)]
    command: Commands,
}

fn main() -> Result<()> {
    color_eyre::config::HookBuilder::default()
        .display_env_section(false)
        .install()?;

    let cli = Cli::parse();

    if cli.debug {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(std::io::stderr)
            .init();
    }

    tracing::debug!("Starting jellycat");

    match &cli.command {
        Commands::Init(args) => {
            return jellycat::commands::init::run(args);
        }
        Commands::Config => {
            return jellycat::commands::config::run();
        }
        Commands::Skills => {
            print!("{}", include_str!("../SKILLS.md"));
            return Ok(());
        }
        Commands::Completions { shell } => {
            generate(*shell, &mut Cli::command(), "jc", &mut std::io::stdout());
            return Ok(());
        }
        _ => {} // Continue for commands that need config
    }

    let repo_root = repo::find_root().ok_or_else(|| {
        eyre::eyre!("Not a jujutsu repository (or any of the parent directories): .jj")
    })?;

    let mut config = config::load(&repo_root).context("Error loading config")?;

    for (old_key, new_key) in &config.deprecated_keys {
        eprintln!(
            "warning: config key '{}' is deprecated, use '{}' instead",
            old_key, new_key
        );
    }
    if !config.deprecated_keys.is_empty() {
        eprintln!("Run 'jc init --force' to reconfigure.");
    }

    // Construct the PR store and populate the prs cache.
    let jj = Arc::new(Jj::new(repo_root.clone()));
    let pr_store = jellycat::pr_store::create(&config.pr_store_type, Arc::clone(&jj));
    config.prs = pr_store.list()?;

    // Commands that don't require upstream_repo configured.
    match &cli.command {
        Commands::Link(args) => {
            return jellycat::commands::link::run(args, &config, pr_store.as_ref());
        }
        Commands::Unlink(args) => {
            return jellycat::commands::unlink::run(args, &config, pr_store.as_ref());
        }
        _ => {}
    }

    if config.upstream_repo.is_none() {
        return Err(eyre!(
            "jellycat upstream not configured. Please run `jc init`."
        ));
    }

    match &cli.command {
        Commands::Submit(args) => jellycat::commands::submit::run(args, &config, pr_store.as_ref()),
        Commands::Status(args) => jellycat::commands::status::run(args, &config),
        Commands::Tidy(args) => jellycat::commands::tidy::run(args, &config, pr_store.as_ref()),
        Commands::Get(args) => jellycat::commands::get::run(args, &config, pr_store.as_ref()),
        Commands::Init(_)
        | Commands::Link(_)
        | Commands::Unlink(_)
        | Commands::Config
        | Commands::Skills
        | Commands::Completions { .. } => unreachable!(),
    }
}
