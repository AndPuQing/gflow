use std::error::Error;
pub mod cancel;
pub mod hold;
pub mod info;
pub mod list;
pub mod log;
pub mod priority;
pub mod restart;
pub mod resume;
pub mod start;
pub mod status;
pub mod stop;
pub mod submit;
use common::arg::Commands;

pub async fn handle_exec(commands: Commands) -> Result<(), Box<dyn Error>> {
    match commands {
        Commands::Submit(submit_args) => todo!(),
        Commands::Status(status_args) => todo!(),
        Commands::Cancel(cancel_args) => todo!(),
        Commands::List(list_args) => todo!(),
        Commands::Log(log_args) => todo!(),
        Commands::Priority(priority_args) => todo!(),
        Commands::Hold(hold_args) => todo!(),
        Commands::Resume(resume_args) => todo!(),
        Commands::Info(info_args) => todo!(),
        Commands::Start(start_args) => start::start(start_args).await?,
        Commands::Stop(stop_args) => todo!(),
        Commands::Restart(restart_args) => todo!(),
    }
    Ok(())
}
