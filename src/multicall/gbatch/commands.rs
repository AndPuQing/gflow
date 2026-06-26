use super::cli::Commands;
use clap::CommandFactory;

pub mod add;
mod new;

pub async fn handle_commands(_: &gflow::config::Config, commands: Commands) -> anyhow::Result<()> {
    match commands {
        Commands::New(new_args) => new::handle_new(new_args),
        Commands::Completion { shell } => {
            crate::multicall::completion::handle_completion(
                shell,
                super::cli::GBatch::command(),
                "gbatch",
            )?;
            Ok(())
        }
    }
}
