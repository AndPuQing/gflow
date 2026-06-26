use anyhow::Result;
use std::io::Write;

pub fn generate_to_stdout(
    shell: clap_complete::Shell,
    cmd: &mut clap::Command,
    bin_name: &str,
) -> Result<()> {
    let mut buf = Vec::<u8>::new();
    clap_complete::generate(shell, cmd, bin_name, &mut buf);

    match std::io::stdout().write_all(&buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Convenience wrapper: build the command from the CLI struct, generate
/// completions, and write them to stdout. Every multicall subcommand's
/// `Completion { shell }` arm delegates here.
pub fn handle_completion(
    shell: clap_complete::Shell,
    mut cmd: clap::Command,
    bin_name: &str,
) -> Result<()> {
    generate_to_stdout(shell, &mut cmd, bin_name)
}
