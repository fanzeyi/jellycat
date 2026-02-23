use clap::Args;

#[derive(Args, Debug)]
pub struct StatusArgs {
    #[arg(short, long)]
    pub revision: Option<String>,
}

pub fn run(args: &StatusArgs) {
    println!("Status command with revision: {:?}", args.revision);
}
