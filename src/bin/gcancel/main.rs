mod cli;

use anyhow::{Context, Result};
use clap::Parser;
use gflow::{client::Client, config::load_config, core::job::JobState};
use range_parser::parse;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GCancel::parse();
    let config = load_config(args.config.as_ref())?;
    let client = Client::build(&config)?;

    let job_ids = parse_job_ids(&args.ids)?;

    if args.dry_run {
        perform_dry_run(&client, &job_ids).await?;
    } else {
        for job_id in &job_ids {
            client.cancel_job(*job_id).await?;
            println!("Job {} cancelled.", job_id);
        }
    }

    Ok(())
}

/// Parse job IDs from string inputs, supporting ranges like "1-3"
fn parse_job_ids(id_strings: &String) -> Result<Vec<u32>> {
    let mut parsed_ids: Vec<u32> =
        parse::<u32>(id_strings.trim()).context(format!("Invalid ID or range: {}", id_strings))?;

    parsed_ids.sort_unstable();
    parsed_ids.dedup();

    Ok(parsed_ids)
}

async fn perform_dry_run(client: &Client, job_ids: &[u32]) -> Result<()> {
    println!("=== Dry Run: Job Cancellation Analysis ===\n");

    for (index, &job_id) in job_ids.iter().enumerate() {
        if index > 0 {
            println!("\n{}", "=".repeat(50));
        }

        // Get the job to be cancelled
        let job = client
            .get_job(job_id)
            .await?
            .context(format!("Job {} not found", job_id))?;

        println!("Job ID: {}", job_id);
        println!("Current State: {}", job.state);

        // Determine if the cancellation is valid
        let can_cancel = job.state.clone().can_transition_to(JobState::Cancelled);

        if can_cancel {
            println!("Target State: {}", JobState::Cancelled);
            println!("Transition: {} → {}", job.state, JobState::Cancelled);

            // Add specific notes based on current state
            match job.state {
                JobState::Running => {
                    println!(
                        "\nNote: Job is currently running. Cancellation will send Ctrl-C to gracefully interrupt it."
                    );
                }
                JobState::Queued => {
                    println!("\nNote: Job is queued and will be removed from the queue.");
                }
                _ => {}
            }
        } else {
            println!("Target State: {} (INVALID)", JobState::Cancelled);
            println!("Transition: {} → {} ✗", job.state, JobState::Cancelled);
            println!("\nError: Cannot cancel a job in '{}' state.", job.state);
            println!("Jobs can only be cancelled from 'Queued' or 'Running' states.");
            continue;
        }

        // Find dependent jobs
        let all_jobs = client.list_jobs().await?;
        let dependent_jobs: Vec<_> = all_jobs
            .iter()
            .filter(|j| j.depends_on == Some(job_id))
            .collect();

        if !dependent_jobs.is_empty() {
            println!("\n=== Chain Reaction: Dependent Jobs ===");
            println!(
                "\nFound {} job(s) that depend on job {}:",
                dependent_jobs.len(),
                job_id
            );

            for dep_job in &dependent_jobs {
                println!("\n  Job ID: {}", dep_job.id);
                println!("  Current State: {}", dep_job.state);
                println!("  Depends On: Job {}", job_id);

                match dep_job.state {
                    JobState::Queued => {
                        println!("  Impact: ⚠ This job will remain queued indefinitely");
                        println!("          (dependency will never complete)");
                    }
                    JobState::Running => {
                        println!("  Impact: ✓ Already running (unaffected)");
                    }
                    JobState::Finished | JobState::Failed | JobState::Cancelled => {
                        println!("  Impact: ✓ Already completed (unaffected)");
                    }
                }
            }

            println!(
                "\n⚠ Warning: {} queued dependent job(s) will be blocked.",
                dependent_jobs
                    .iter()
                    .filter(|j| j.state == JobState::Queued)
                    .count()
            );
            println!("  Consider cancelling these jobs as well if they are no longer needed.");
        } else {
            println!("\n=== Chain Reaction: Dependent Jobs ===");
            println!("\n✓ No jobs depend on job {}.", job_id);
            println!("  Cancellation will not affect any other jobs.");
        }

        println!("\n=== Summary ===");
        println!(
            "Would cancel job {} ({} → {})",
            job_id,
            job.state,
            JobState::Cancelled
        );
    }

    Ok(())
}
