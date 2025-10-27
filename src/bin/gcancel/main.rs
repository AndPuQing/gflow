mod cli;

use anyhow::{Context, Result};
use clap::Parser;
use gflow::{client::Client, config::load_config, core::job::JobState};

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GCancel::parse();
    let config = load_config(args.config.as_ref())?;
    let client = Client::build(&config)?;

    if args.dry_run {
        perform_dry_run(&client, args.id).await?;
    } else {
        client.cancel_job(args.id).await?;
        println!("Job {} cancelled.", args.id);
    }

    Ok(())
}

async fn perform_dry_run(client: &Client, job_id: u32) -> Result<()> {
    println!("=== Dry Run: Job Cancellation Analysis ===\n");

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
        println!("Valid Transition: ✓ Yes");

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
        println!("Valid Transition: ✗ No");
        println!("\nError: Cannot cancel a job in '{}' state.", job.state);
        println!("Jobs can only be cancelled from 'Queued' or 'Running' states.");
        return Ok(());
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

    Ok(())
}
