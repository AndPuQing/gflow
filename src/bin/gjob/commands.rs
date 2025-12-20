use crate::cli::Commands;
use clap::CommandFactory;
use clap_complete::generate;

pub mod attach;
pub mod close_sessions;
pub mod hold;
pub mod log;
pub mod redo;
pub mod release;
pub mod show;

pub async fn handle_commands(
    config_path: &Option<std::path::PathBuf>,
    command: Commands,
) -> anyhow::Result<()> {
    match command {
        Commands::Attach { job } => {
            attach::handle_attach(config_path, &job).await?;
        }
        Commands::Log { job } => {
            log::handle_log(config_path, &job).await?;
        }
        Commands::Hold { job } => {
            hold::handle_hold(config_path, job).await?;
        }
        Commands::Release { job } => {
            release::handle_release(config_path, job).await?;
        }
        Commands::Show { job } => {
            show::handle_show(config_path, job).await?;
        }
        Commands::Redo {
            job,
            gpus,
            priority,
            depends_on,
            time,
            memory,
            conda_env,
            clear_deps,
        } => {
            redo::handle_redo(
                config_path,
                &job,
                gpus,
                priority,
                depends_on,
                time,
                memory,
                conda_env,
                clear_deps,
            )
            .await?;
        }
        Commands::CloseSessions {
            jobs,
            state,
            pattern,
            all,
        } => {
            close_sessions::handle_close_sessions(config_path, &jobs, &state, &pattern, all)
                .await?;
        }
        Commands::Completion { shell } => {
            let mut cmd = crate::cli::GJob::command();
            generate(shell, &mut cmd, "gjob", &mut std::io::stdout());
        }
    }

    Ok(())
}
