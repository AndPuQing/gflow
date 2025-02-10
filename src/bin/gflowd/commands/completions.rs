use crate::cli::{self, GFlow};
use clap::CommandFactory;
use clap_complete::{generate, shells::*};
use std::io;

pub(crate) fn handle_completions(completions_args: cli::CompletionsArgs) {
    let mut cmd = GFlow::command();
    match completions_args.shell {
        cli::Shell::Bash => generate(Bash, &mut cmd, "gflow", &mut io::stdout()),
        cli::Shell::Elvish => generate(Elvish, &mut cmd, "gflow", &mut io::stdout()),
        cli::Shell::Fish => generate(Fish, &mut cmd, "gflow", &mut io::stdout()),
        cli::Shell::Powershell => generate(PowerShell, &mut cmd, "gflow", &mut io::stdout()),
        cli::Shell::Zsh => generate(Zsh, &mut cmd, "gflow", &mut io::stdout()),
    }
}
