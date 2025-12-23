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
    // Load config and create client
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    // Collect session names to close
    let mut sessions_to_close = HashSet::new();

    // If --all flag is set, get all gflow sessions
    if all {
        let all_sessions = tmux::get_all_session_names();
        // Filter for gflow-managed sessions (those that start with "gflow-")
        for session in all_sessions {
            if session.starts_with("gflow-") {
                sessions_to_close.insert(session);
            }
        }
    }

    // Parse job IDs if provided
    let job_ids = if let Some(ids_str) = job_ids_str {
        Some(parse_job_ids(ids_str)?)
    } else {
        None
    };

    // If job_ids, states, or pattern are specified, query jobs from daemon
    if job_ids.is_some() || states.is_some() || pattern.is_some() {
        let jobs = client.list_jobs().await?;

        for job in jobs {
            let session_name = match &job.run_name {
                Some(name) => name,
                None => continue, // Skip jobs without tmux sessions
            };

            // Filter by job IDs
            if let Some(ref ids) = job_ids {
                if ids.contains(&job.id) {
                    sessions_to_close.insert(session_name.clone());
                    continue;
                }
            }

            // Filter by states
            if let Some(state_filter) = states {
                if state_filter.contains(&job.state) {
                    sessions_to_close.insert(session_name.clone());
                    continue;
                }
            }

            // Filter by pattern
            if let Some(pat) = pattern {
                if session_name.contains(pat.as_str()) {
                    sessions_to_close.insert(session_name.clone());
                }
            }
        }
    }

    // Check if we have any sessions to close
    if sessions_to_close.is_empty() {
        println!("No tmux sessions found matching the specified criteria.");
        return Ok(());
    }

    // Convert to sorted vector for consistent output
    let mut sessions_vec: Vec<String> = sessions_to_close.into_iter().collect();
    sessions_vec.sort();

    // Show what we're about to close
    println!("Closing {} tmux session(s):", sessions_vec.len());
    for session in &sessions_vec {
        println!("  - {}", session);
    }

    // Close sessions in batch
    let results = tmux::kill_sessions_batch(&sessions_vec);

    // Report results
    let mut success_count = 0;
    let mut failed_count = 0;

    for (session_name, result) in results {
        match result {
            Ok(_) => {
                success_count += 1;
            }
            Err(e) => {
                eprintln!("Failed to close session '{}': {}", session_name, e);
                failed_count += 1;
            }
        }
    }

    println!("\nClosed {} session(s) successfully.", success_count);
    if failed_count > 0 {
        eprintln!("Failed to close {} session(s).", failed_count);
    }

    Ok(())
}
