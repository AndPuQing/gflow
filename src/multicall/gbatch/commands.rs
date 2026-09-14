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
        // Query-word aliases never reach here: `run` forwards them to the
        // command that owns the query before loading any config.
        Commands::List(_) | Commands::Queue(_) | Commands::Status(_) | Commands::Log(_) => {
            unreachable!("query aliases are forwarded before command handling")
        }
    }
}
