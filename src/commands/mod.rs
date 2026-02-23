use clap::Subcommand;

pub mod status;
pub mod submit;

#[derive(Subcommand, Debug)]
pub enum Commands {
    Submit(submit::SubmitArgs),
    Status(status::StatusArgs),
}
