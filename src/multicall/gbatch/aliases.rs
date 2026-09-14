//! Query-word aliases for `gbatch`.
//!
//! `gbatch` is a submitter; `status` / `log` / `list` / `queue` are query words
//! users carry over from `squeue` / `scontrol`. Before this module existed they
//! were swallowed as `script_or_command` and each one silently submitted a job
//! that failed a second later (W-576). They are now forwarded to the command
//! that actually owns the query.

use super::cli::Commands;
use std::ffi::OsString;
use std::path::Path;

/// Multiclall program an alias subcommand forwards to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasTarget {
    GQueue,
    GJob,
}

impl AliasTarget {
    pub fn program_name(self) -> &'static str {
        match self {
            AliasTarget::GQueue => "gqueue",
            AliasTarget::GJob => "gjob",
        }
    }
}

/// Map a `gbatch` alias subcommand to the lookup command that owns it.
///
/// Returns `None` for real `gbatch` subcommands (`new`, `completion`), which
/// are handled by the submit path. The returned argv excludes the program name
/// and starts with the target's own subcommand/arguments.
pub fn resolve_alias(
    command: &Commands,
    config: Option<&Path>,
) -> Option<(AliasTarget, Vec<OsString>)> {
    let (target, mut argv) = match command {
        Commands::List(args) | Commands::Queue(args) => (
            AliasTarget::GQueue,
            args.args.iter().map(OsString::from).collect::<Vec<_>>(),
        ),
        Commands::Status(args) => (
            AliasTarget::GJob,
            vec![OsString::from("show"), OsString::from(&args.job)],
        ),
        Commands::Log(args) => {
            let mut argv = vec![OsString::from("log"), OsString::from(&args.job)];
            if let Some(first) = args.first {
                argv.push(OsString::from("--first"));
                argv.push(OsString::from(first.get().to_string()));
            }
            if let Some(last) = args.last {
                argv.push(OsString::from("--last"));
                argv.push(OsString::from(last.get().to_string()));
            }
            (AliasTarget::GJob, argv)
        }
        _ => return None,
    };

    // `--config` is a hidden global on gbatch: keep it effective for the
    // forwarded command so both halves honour the same config file.
    if let Some(path) = config {
        argv.push(OsString::from("--config"));
        argv.push(path.as_os_str().to_os_string());
    }

    Some((target, argv))
}

/// Build the argv the target multicall program expects (program name first).
pub fn forwarded_argv(target: AliasTarget, rest: Vec<OsString>) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(rest.len() + 1);
    argv.push(OsString::from(target.program_name()));
    argv.extend(rest);
    argv
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multicall::gbatch::cli::GBatch;
    use clap::Parser;
    use std::path::PathBuf;

    fn alias_of(argv: &[&str]) -> (AliasTarget, Vec<OsString>) {
        let parsed = GBatch::try_parse_from(argv).expect("should parse");
        let command = parsed.commands.expect("expected a subcommand");
        resolve_alias(&command, None).expect("should resolve to an alias")
    }

    #[test]
    fn list_and_queue_forward_to_gqueue() {
        let (target, argv) = alias_of(&["gbatch", "list", "-a", "-u", "all"]);
        assert_eq!(target, AliasTarget::GQueue);
        assert_eq!(argv, ["-a", "-u", "all"].map(OsString::from));

        let (target, argv) = alias_of(&["gbatch", "queue"]);
        assert_eq!(target, AliasTarget::GQueue);
        assert!(argv.is_empty());
    }

    #[test]
    fn status_forwards_to_gjob_show() {
        let (target, argv) = alias_of(&["gbatch", "status", "343"]);
        assert_eq!(target, AliasTarget::GJob);
        assert_eq!(argv, vec![OsString::from("show"), OsString::from("343")]);
    }

    #[test]
    fn log_forwards_to_gjob_log_with_slice_flags() {
        let (target, argv) = alias_of(&["gbatch", "log", "343", "--last", "50"]);
        assert_eq!(target, AliasTarget::GJob);
        assert_eq!(argv, ["log", "343", "--last", "50"].map(OsString::from));
    }

    #[test]
    fn config_is_forwarded_to_the_target() {
        let parsed =
            GBatch::try_parse_from(["gbatch", "--config", "/tmp/custom.toml", "list"]).unwrap();
        let command = parsed.commands.unwrap();
        let (target, argv) = resolve_alias(&command, parsed.config.as_deref()).unwrap();

        assert_eq!(target, AliasTarget::GQueue);
        assert_eq!(argv, ["--config", "/tmp/custom.toml"].map(OsString::from));
    }

    #[test]
    fn submit_subcommands_are_not_aliases() {
        let parsed = GBatch::try_parse_from(["gbatch", "new", "train"]).unwrap();
        let command = parsed.commands.unwrap();
        assert!(resolve_alias(&command, None).is_none());
    }

    #[test]
    fn forwarded_argv_prepends_program_name() {
        let argv = forwarded_argv(AliasTarget::GJob, vec![OsString::from("show")]);
        assert_eq!(argv, vec![OsString::from("gjob"), OsString::from("show")]);
    }

    #[test]
    fn forwarded_argv_accepts_path_config() {
        let (_, argv) = resolve_alias(
            &GBatch::try_parse_from(["gbatch", "list"])
                .unwrap()
                .commands
                .unwrap(),
            Some(&PathBuf::from("/etc/gflow.toml")),
        )
        .unwrap();
        assert_eq!(argv, ["--config", "/etc/gflow.toml"].map(OsString::from));
    }
}
