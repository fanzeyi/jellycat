use crate::config::Config;
use clap::Args;

#[derive(Args, Debug)]
pub struct SubmitArgs {
    #[arg(short, long)]
    pub revision: Option<String>,
}

pub fn run(args: &SubmitArgs, config: &Config) {
    println!("Submit command with revision: {:?}", args.revision);
    println!("Config: {:?}", config);
}
