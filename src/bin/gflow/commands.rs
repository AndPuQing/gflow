use crate::cli::{Commands, DaemonCommands, JobCommands};
use fail::handle_fail;

mod add;
mod completions;
mod fail;
mod finish;
mod list;
mod logs;
mod new;
mod start;
mod status;
mod stop;

pub async fn handle_commands(config: &config::Config, commands: Commands) -> anyhow::Result<()> {
    match commands {
        Commands::Add(add_args) => add::handle_add(config, add_args).await,
        Commands::List(list_args) => list::handle_list(config, list_args).await,
        Commands::Completions(completions_args) => {
            completions::handle_completions(completions_args)
        }
        Commands::Daemon(daemon_command) => match daemon_command {
            DaemonCommands::Start => start::handle_start(),
            DaemonCommands::Stop => stop::handle_stop(),
            DaemonCommands::Status => status::handle_status(),
        },
        Commands::Job(job_command) => match job_command {
            JobCommands::Finish(finish_args) => finish::handle_finish(config, finish_args).await,
            JobCommands::Fail(fail_args) => handle_fail(config, fail_args).await,
            JobCommands::Logs(logs_args) => logs::handle_logs(logs_args).await,
        },
        Commands::New(new_args) => new::handle_new(new_args),
    }
}
