use clap::Subcommand;

pub mod init;
pub mod link;
pub mod status;
pub mod submit;
pub mod tidy;
pub mod unlink;

#[derive(Subcommand, Debug)]
pub enum Commands {
    Submit(submit::SubmitArgs),
    Status(status::StatusArgs),
    Init(init::InitArgs),
    Link(link::LinkArgs),
    Unlink(unlink::UnlinkArgs),
    Tidy(tidy::TidyArgs),
}
