use crate::cli::Commands;
mod add;
mod completions;
mod start;
mod stop;

pub fn handle_commands(commands: Commands) {
    match commands {
        Commands::Add(add_args) => add::handle_add(add_args),
        Commands::Completions(completions_args) => {
            completions::handle_completions(completions_args)
        }
        Commands::Start => start::handle_start(),
        Commands::Stop => stop::handle_stop(),
    }
}
