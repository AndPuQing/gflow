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
pub mod update;

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
        Commands::Update {
            job,
            command,
            script,
            gpus,
            conda_env,
            clear_conda_env,
            priority,
            time_limit,
            clear_time_limit,
            memory_limit,
            clear_memory_limit,
            depends_on,
            depends_on_all,
            depends_on_any,
            auto_cancel_on_dep_failure,
            no_auto_cancel_on_dep_failure,
            max_concurrent,
            clear_max_concurrent,
            params,
        } => {
            let update_params = update::UpdateJobParams {
                job_ids_str: job,
                command,
                script,
                gpus,
                conda_env,
                clear_conda_env,
                priority,
                time_limit,
                clear_time_limit,
                memory_limit,
                clear_memory_limit,
                depends_on,
                depends_on_all,
                depends_on_any,
                auto_cancel_on_dep_failure,
                no_auto_cancel_on_dep_failure,
                max_concurrent,
                clear_max_concurrent,
                params,
            };
            update::handle_update(config_path, update_params).await?;
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
            cascade,
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
                cascade,
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
