use crate::cli::Commands;

pub mod add;
mod completions;
mod new;

pub async fn handle_commands(_: &config::Config, commands: Commands) -> anyhow::Result<()> {
    match commands {
        Commands::Completions(completions_args) => {
            completions::handle_completions(completions_args)
        }
        Commands::New(new_args) => new::handle_new(new_args),
    }
}
