use anyhow::Result;
use gflow::{core::job::JobState, tmux, utils::parse_job_ids};
use std::collections::HashSet;

fn is_missing_session_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("does not exist")
        || message.contains("can't find session")
        || message.contains("unknown session")
}

pub async fn handle_close_sessions(
    config_path: &Option<std::path::PathBuf>,
    job_ids_str: &Option<String>,
    states: &Option<Vec<JobState>>,
    pattern: &Option<String>,
    all: bool,
) -> Result<()> {
    let mut sessions_to_close = HashSet::new();

    // For -a flag, get existing tmux sessions and exclude running jobs
    if all {
        let mut gflow_sessions: HashSet<_> = tmux::get_all_session_names()
            .into_iter()
            .filter(|s| s.starts_with("gflow-job-"))
            .collect();

        if gflow_sessions.is_empty() {
            eprintln!("gjob close-sessions: no sessions to close");
            return Ok(());
        }

        // Exclude running job sessions - only query Running state for efficiency
        let client = gflow::create_client(config_path)?;
        let running_jobs = client
            .list_jobs_with_query(Some("Running".to_string()), None, None, None, None)
            .await?;
        for job in running_jobs {
            if let Some(name) = job.run_name {
                gflow_sessions.remove(name.as_str());
            }
        }

        if gflow_sessions.is_empty() {
            eprintln!("gjob close-sessions: no sessions to close");
            return Ok(());
        }

        sessions_to_close = gflow_sessions;
    } else {
        let client = gflow::create_client(config_path)?;

        let jobs = client.list_jobs().await?;

        let job_ids = match job_ids_str {
            Some(s) => Some(parse_job_ids(s)?),
            None => None,
        };

        for job in &jobs {
            let Some(session_name) = &job.run_name else {
                continue;
            };

            if job_ids.is_none() && states.is_none() && pattern.is_none() {
                continue;
            }

            let matches = job_ids.as_ref().is_none_or(|ids| ids.contains(&job.id))
                && states.as_ref().is_none_or(|ss| ss.contains(&job.state))
                && pattern
                    .as_ref()
                    .is_none_or(|pat| session_name.contains(pat));

            if matches && (states.is_some() || job.state.is_final()) {
                sessions_to_close.insert(session_name.to_string());
            }
        }

        if sessions_to_close.is_empty() {
            if job_ids_str.is_none() && states.is_none() && pattern.is_none() {
                eprintln!(
                    "gjob close-sessions: no filters specified (see gjob close-sessions --help)"
                );
            } else {
                eprintln!("gjob close-sessions: no matching sessions");
            }
            return Ok(());
        }
    }

    let mut sessions: Vec<_> = sessions_to_close.into_iter().collect();
    sessions.sort();

    eprintln!("gjob close-sessions: closing {} session(s)", sessions.len());
    for s in &sessions {
        println!("{}", s);
    }

    let results = tmux::kill_sessions_batch(&sessions);

    let mut ok = 0;
    let mut already_gone = 0;
    let mut failed = 0;

    for (name, res) in results {
        match res {
            Ok(_) => ok += 1,
            Err(e) => {
                if is_missing_session_error(&e.to_string()) {
                    already_gone += 1;
                } else {
                    eprintln!(
                        "gjob close-sessions: failed to close session '{}': {}",
                        name, e
                    );
                    failed += 1;
                }
            }
        }
    }

    eprintln!(
        "gjob close-sessions: Closed={} AlreadyClosed={} Failed={}",
        ok, already_gone, failed
    );

    Ok(())
}
