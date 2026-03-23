use anyhow::Context;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use jellycat::commands::Commands;
use jellycat::config;
use jellycat::repo;

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

fn main() -> anyhow::Result<()> {
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
        Commands::Link(args) => {
            return jellycat::commands::link::run(args);
        }
        Commands::Unlink(args) => {
            return jellycat::commands::unlink::run(args);
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
        anyhow::anyhow!("Not a jujutsu repository (or any of the parent directories): .jj")
    })?;

    let config = config::load(&repo_root).context("Error loading config")?;

    for (old_key, new_key) in &config.deprecated_keys {
        eprintln!(
            "warning: config key '{}' is deprecated, use '{}' instead",
            old_key, new_key
        );
    }
    if !config.deprecated_keys.is_empty() {
        eprintln!("Run 'jc init --force' to reconfigure.");
    }

    if config.upstream_repo.is_none() {
        return Err(anyhow::anyhow!(
            "jellycat upstream not configured. Please run `jc init`."
        ));
    }

    match &cli.command {
        Commands::Submit(args) => jellycat::commands::submit::run(args, &config),
        Commands::Status(args) => jellycat::commands::status::run(args, &config),
        Commands::Tidy(args) => jellycat::commands::tidy::run(args, &config),
        Commands::Get(args) => jellycat::commands::get::run(args, &config),
        Commands::Init(_)
        | Commands::Link(_)
        | Commands::Unlink(_)
        | Commands::Skills
        | Commands::Completions { .. } => unreachable!(),
    }
}
