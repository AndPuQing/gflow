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
        Commands::Submit(_submit_args) => todo!(),
        Commands::Status(_status_args) => todo!(),
        Commands::Cancel(_cancel_args) => todo!(),
        Commands::List(_list_args) => todo!(),
        Commands::Log(_log_args) => todo!(),
        Commands::Priority(_priority_args) => todo!(),
        Commands::Hold(_hold_args) => todo!(),
        Commands::Resume(_resume_args) => todo!(),
        Commands::Info(_info_args) => todo!(),
        Commands::Start(start_args) => start::start(start_args).await?,
        Commands::Stop(_stop_args) => todo!(),
        Commands::Restart(_restart_args) => todo!(),
    }
    Ok(())
}
