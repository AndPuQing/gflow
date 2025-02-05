use clap::{CommandFactory, Parser};
use clap_complete::{generate, shells::*};
use cli::GFlow;
use std::io;
mod cli;
mod help;

fn main() {
    let gflow = GFlow::parse();
    env_logger::Builder::new()
        .filter_level(gflow.verbose.log_level_filter())
        .init();

    log::debug!("{:?}", gflow);

    match gflow.commands {
        Some(cli::Commands::Add(add_args)) => {
            log::debug!("{:?}", add_args);
            println!("Adding job: {:?}", add_args.script);
        }
        Some(cli::Commands::Completions(completions_args)) => {
            let mut cmd = GFlow::command();
            match completions_args.shell {
                cli::Shell::Bash => generate(Bash, &mut cmd, "gflow", &mut io::stdout()),
                cli::Shell::Elvish => generate(Elvish, &mut cmd, "gflow", &mut io::stdout()),
                cli::Shell::Fish => generate(Fish, &mut cmd, "gflow", &mut io::stdout()),
                cli::Shell::Powershell => {
                    generate(PowerShell, &mut cmd, "gflow", &mut io::stdout())
                }
                cli::Shell::Zsh => generate(Zsh, &mut cmd, "gflow", &mut io::stdout()),
            }
        }
        None => {}
    }
}
