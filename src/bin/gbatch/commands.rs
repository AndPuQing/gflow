use crate::cli::Commands;
use clap::CommandFactory;
use clap_complete::generate;

pub mod add;
mod new;

pub async fn handle_commands(_: &gflow::config::Config, commands: Commands) -> anyhow::Result<()> {
    match commands {
        Commands::New(new_args) => new::handle_new(new_args),
        Commands::Completion { shell } => {
            let mut cmd = crate::cli::GBatch::command();
            generate(shell, &mut cmd, "gbatch", &mut std::io::stdout());
            Ok(())
        }
    }
}
