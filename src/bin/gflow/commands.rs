use fail::handle_fail;

use crate::cli::Commands;
mod add;
mod completions;
mod fail;
mod finish;
mod list;
mod logs;
mod start;
mod stop;

pub async fn handle_commands(config: &config::Config, commands: Commands) -> anyhow::Result<()> {
    match commands {
        Commands::Submit(submit_args) => add::handle_submit(config, submit_args).await,
        Commands::Completions(completions_args) => {
            completions::handle_completions(completions_args)
        }
        Commands::Up => start::handle_start(),
        Commands::Stop => stop::handle_stop(),
        Commands::Finish(finish_args) => finish::handle_finish(config, finish_args).await,
        Commands::List(list_args) => list::handle_list(config, list_args).await,
        Commands::Fail(fail_args) => handle_fail(config, fail_args).await,
        Commands::Logs(logs_args) => logs::handle_logs(logs_args).await,
    }
}
