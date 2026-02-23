use clap::Args;

#[derive(Args, Debug)]
pub struct SubmitArgs {
    #[arg(short, long)]
    pub revision: Option<String>,
}

pub fn run(args: &SubmitArgs) {
    println!("Submit command with revision: {:?}", args.revision);
}
