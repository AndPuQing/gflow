use std::io::Write;

pub fn generate_to_stdout(
    shell: clap_complete::Shell,
    cmd: &mut clap::Command,
    bin_name: &str,
) -> anyhow::Result<()> {
    let mut buf = Vec::<u8>::new();
    clap_complete::generate(shell, cmd, bin_name, &mut buf);

    match std::io::stdout().write_all(&buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e.into()),
    }
}
