use jellycat::commands::Commands;
use jellycat::config;
use jellycat::repo;
use anyhow::Context;
use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
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
        _ => {} // Continue for commands that need config
    }

    let repo_root = repo::find_root()
        .ok_or_else(|| anyhow::anyhow!("Not a jujutsu repository (or any of the parent directories): .jj"))?;

    let config = config::load(&repo_root).context("Error loading config")?;

    if config.upstream.is_none() {
        return Err(anyhow::anyhow!("jellycat upstream not configured. Please run `jellycat init`."));
    }

    match &cli.command {
        Commands::Submit(args) => jellycat::commands::submit::run(args, &config),
        Commands::Status(args) => jellycat::commands::status::run(args, &config),
        Commands::Tidy(args) => jellycat::commands::tidy::run(args, &config),
        Commands::Init(_) | Commands::Link(_) | Commands::Unlink(_) => unreachable!(),
    }
}
