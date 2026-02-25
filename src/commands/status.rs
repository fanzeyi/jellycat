use crate::config::Config;
use clap::Args;

#[derive(Args, Debug)]
pub struct StatusArgs {
    #[arg(short, long)]
    pub revision: Option<String>,
}

pub fn run(args: &StatusArgs, config: &Config) -> anyhow::Result<()> {
    println!("Status command with revision: {:?}", args.revision);
    println!("Config: {:?}", config);
    Ok(())
}
