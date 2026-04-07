use clap::Subcommand;
use clap_complete::Shell;

pub mod config;
pub mod context;
pub mod get;
pub mod init;
pub mod link;
pub mod status;
pub mod submit;
pub mod pull;
pub mod tidy;
pub mod unlink;

pub use context::CommandCtx;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Push commits and create or update GitHub pull requests
    Submit(submit::SubmitArgs),
    /// Show the status of pull requests for the current stack
    Status(status::StatusArgs),
    /// Initialize jellycat configuration for the current repository
    Init(init::InitArgs),
    /// Associate a commit with an existing pull request
    Link(link::LinkArgs),
    /// Remove the pull request association from a commit
    Unlink(unlink::UnlinkArgs),
    /// Pull PR titles and bodies into local commit descriptions
    Pull(pull::PullArgs),
    /// Clean up merged bookmarks and their remote tracking branches
    Tidy(tidy::TidyArgs),
    /// Print jellycat configuration for the current repository
    Config,
    /// Fetch a pull request branch from GitHub
    Get(get::GetArgs),
    /// Print AI assistant skills reference for teaching AI tools to use jc
    Skills,
    /// Generate shell completions
    Completions {
        /// The shell to generate completions for
        shell: Shell,
    },
}
