use anyhow::Result;
use gflow::{client::Client, core::job::JobState, tmux, utils::parse_job_ids};
use std::collections::HashSet;

pub async fn handle_close_sessions(
    config_path: &Option<std::path::PathBuf>,
    job_ids_str: &Option<String>,
    states: &Option<Vec<JobState>>,
    pattern: &Option<String>,
    all: bool,
) -> Result<()> {
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    let jobs = client.list_jobs().await?;
    let mut sessions_to_close = HashSet::new();

    let job_ids = match job_ids_str {
        Some(s) => Some(parse_job_ids(s)?),
        None => None,
    };

    for job in &jobs {
        let Some(session_name) = &job.run_name else {
            continue;
        };

        if all && job.state.is_final() {
            sessions_to_close.insert(session_name.clone());
            continue;
        }

        if job_ids.is_none() && states.is_none() && pattern.is_none() {
            continue;
        }

        let matches = job_ids.as_ref().is_none_or(|ids| ids.contains(&job.id))
            && states.as_ref().is_none_or(|ss| ss.contains(&job.state))
            && pattern
                .as_ref()
                .is_none_or(|pat| session_name.contains(pat));

        if matches && (states.is_some() || job.state.is_final()) {
            sessions_to_close.insert(session_name.clone());
        }
    }

    if sessions_to_close.is_empty() {
        println!("No tmux sessions found matching the specified criteria.");
        return Ok(());
    }

    let mut sessions: Vec<_> = sessions_to_close.into_iter().collect();
    sessions.sort();

    println!("Closing {} tmux session(s):", sessions.len());
    for s in &sessions {
        println!("  - {}", s);
    }

    let results = tmux::kill_sessions_batch(&sessions);

    let mut ok = 0;
    let mut failed = 0;

    for (name, res) in results {
        match res {
            Ok(_) => ok += 1,
            Err(e) => {
                eprintln!("Failed to close session '{}': {}", name, e);
                failed += 1;
            }
        }
    }

    println!("\nClosed {} session(s) successfully.", ok);
    if failed > 0 {
        eprintln!("Failed to close {} session(s).", failed);
    }

    Ok(())
}
