use crate::cli::Commands;
mod add;
mod completions;
mod start;
mod stop;

pub async fn handle_commands(commands: Commands) {
    match commands {
        Commands::Add(add_args) => add::handle_add(add_args).await,
        Commands::Completions(completions_args) => {
            completions::handle_completions(completions_args)
        }
        Commands::Up => start::handle_start(),
        Commands::Stop => stop::handle_stop(),
    }
}
