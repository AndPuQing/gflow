use crate::cli::Commands;

mod add;
mod completions;
mod new;

pub async fn handle_commands(config: &config::Config, commands: Commands) -> anyhow::Result<()> {
    match commands {
        Commands::Add(add_args) => add::handle_add(config, add_args).await,
        Commands::Completions(completions_args) => {
            completions::handle_completions(completions_args)
        }
        Commands::Job(job_command) => match job_command {},
        Commands::New(new_args) => new::handle_new(new_args),
    }
}
