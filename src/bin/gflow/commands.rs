use fail::handle_fail;

use crate::cli::Commands;
mod add;
mod completions;
mod fail;
mod finish;
mod list;
mod start;
mod stop;

pub async fn handle_commands(commands: Commands) -> anyhow::Result<()> {
    match commands {
        Commands::Submit(submit_args) => add::handle_submit(submit_args).await,
        Commands::Completions(completions_args) => {
            completions::handle_completions(completions_args)
        }
        Commands::Up => start::handle_start(),
        Commands::Stop => stop::handle_stop(),
        Commands::Finish(finish_args) => finish::handle_finish(finish_args).await,
        Commands::List => list::handle_list().await,
        Commands::Fail(fail_args) => handle_fail(fail_args).await,
    }
}
