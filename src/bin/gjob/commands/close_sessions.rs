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

    // If --all flag is set, get sessions for all completed jobs
    if all {
        let jobs = client.list_jobs().await?;
        for job in jobs {
            // Only close sessions for completed jobs
            if let Some(session_name) = &job.run_name {
                if job.state.is_final() {
                    sessions_to_close.insert(session_name.clone());
                }
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

            let mut should_close = false;

            // Filter by job IDs
            if let Some(ref ids) = job_ids {
                if ids.contains(&job.id) {
                    should_close = true;
                }
            }

            // Filter by states (explicit state selection overrides completed-only default)
            if let Some(state_filter) = states {
                should_close = state_filter.contains(&job.state);
            }

            // Filter by pattern
            if let Some(pat) = pattern {
                if session_name.contains(pat.as_str()) {
                    should_close = true;
                }
            }

            // Only close sessions for completed jobs unless states are explicitly specified
            if should_close && (states.is_some() || job.state.is_final()) {
                sessions_to_close.insert(session_name.clone());
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
